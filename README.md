# QQL — Quran Query Language

A small, portable Rust library that parses compact textual references to Islamic texts and resolves them against local JSON data.

```text
Q:2:1-5,255;Q:1;
```

> Surah 2 ayat 1–5 plus ayah 255, then all of Surah 1.

Usable as an idiomatic Rust crate, or through a C ABI from any language that speaks one — Dart FFI, Python, Go, C/C++ — on Linux, Windows, macOS, Android NDK, and iOS. Not coupled to Flutter.

> **Status:** complete for v1 — parser, source registry, Quran / hadith / Hisnul Muslim / Psalms / Bible / Torah resolvers, CLI, C ABI, and a Dart binding, with 156 Rust tests (196 with both optional features) and 13 Dart tests. See [docs/plan.md](docs/plan.md) for the full design.

## Syntax

```text
query      := reference (';' reference)* ';'?
reference  := source ':' path (':' body)?   // OT:GEN, OT:GEN:1:1
            | (source ':')? body
path       := segment (':' segment)*        // GEN · BI:OT:GEN
segment    := ident | integer ident         // GEN · 1SA, no space inside
body       := text                          // Q:"..."       search
            | ':' selector                  // B::100
            | group (',' group)*
group      := primary ':' text              // Q:1:"..."     search
            | primary ':' scope ':' text    // Q:1:3~5:"..." search
            | primary (':' selector)?
scope      := integer '~' integer
selector   := item (',' item)*
item       := integer | integer '-' integer
text       := quoted                          exact substring
            | '*' quoted ('~' integer)?      similarity, optionally capped
            | '?' quoted ('~' integer)?      full text, optionally capped
quoted     := '"' ... '"' | "'" ... "'"
source     := [A-Za-z][A-Za-z0-9_]*
```

| Part | Meaning |
| --- | --- |
| **source** | Collection code, normalized to uppercase (`Q`, `B`, `M`, `HM`, …). **Optional — omitted means the Quran.** |
| **path** | Book names, for the sources that have books: `GEN` in `OT:GEN:1:1`, `OT:GEN` in `BI:OT:GEN:1:1`. Only right after a stated code. |
| **primary** | First-level index — Surah for Quran, psalm for Psalms, chapter for Hadith, Hisnul Muslim and the Bible's books |
| **selector** | Optional list of items within the primary. Omit it to select everything. |
| `-` | Inclusive range: `1-5` → 1, 2, 3, 4, 5 |
| `,` | Joins items, and joins groups: `1:2,3,2:5` |
| `;` | Separates references. Only needed to switch collection or start a new primary; a trailing one is optional. |
| `::` | Skips the primary: `B::100` numbers across the whole collection. |
| `"…"` / `'…'` | Exact-substring search within whatever the reference scopes. |
| `~` | Scopes a search to an item range: `Q:1:3~5:"…"`, or caps results: `Q:*"…"~5`. |
| `*"…"` `*'…'` | Similarity search, ranked by score. Needs the `vector` feature. |
| `?"…"` `?'…'` | Ranked full-text search with stemming. Needs the `fulltext` feature. |

Whitespace around tokens is accepted: `Q : 2 : 1-5, 255;`

### Groups

One source can address several chapters at once. The rule: **an integer followed by `:` starts a new group**, so it is a primary rather than another selector item.

```text
q:1:2,3,2:3,4-6
```

> Surah 1 ayat 2 and 3, then Surah 2 ayat 3 and 4–6.

```text
Q:1,2:255        all of Surah 1, then Surah 2 ayah 255
Q:1,2,3          three whole Surahs
B:1:1,2:5        Bukhari chapter 1 hadith 1, then chapter 2 hadith 5
```

A range is never a primary, so `Q:1:1-5:3` is a syntax error rather than a second reading. Both things it could have meant are writable, and they differ:

```text
Q:1:1-5;3     Surah 1 ayat 1–5, then all of Surah 3
Q:1:1-5,3     Surah 1 ayat 1–5, plus ayah 3 (already in the range, so deduped)
```

### Omitting the source

Leave the code out and the query is Quran:

```text
1                the whole of Surah 1
2:255            Ayat al-Kursi
1,2:255          all of Surah 1, then Surah 2 ayah 255
1:2,3,2:3,4-6    groups work the same
```

`;` is only needed to change collection, and the last one can always be dropped:

```text
1:1;b:1:1        Quran 1:1, then Bukhari chapter 1 hadith 1
q:1;b:1          same as q:1;b:1;
```

### A stated source carries forward

Once a query names a collection, everything after it belongs to that collection until another code says otherwise:

```text
b:1:1;3          Bukhari 1:1, then Bukhari chapter 3
b:1:1;q:3        Bukhari 1:1, then Surah 3
b:1:1;3;q:1;2    Bukhari 1:1 and 3, then Surah 1 and Surah 2
```

The Quran default applies only when nothing has been named yet — `1,2:255` and the leading `1:1` in `1:1;b:1:1`.

The parser handles the carry-forward, which stays pure syntax: "reuse the previous code" needs no idea what the codes mean. It never learns that `Q` is the default either — it records that no code was stated, and the registry substitutes one when resolving.

### Search

A quoted term searches the text instead of addressing it. The reference in
front of it says *where* to look:

```text
"text"            search the Quran — the default source
q:"text"          search the whole Quran
q:1:"text"        search Surah 1
q:1:3~5:"text"    search ayat 3–5 of Surah 1
b:1:"text"        search Bukhari chapter 1
b:"text"          search all of Bukhari
```

Either quote delimits a term, identically — so a term containing one can
always be written with the other:

```text
q:1:'الحمد'       same as q:1:"الحمد"
b:1:"Allah's"     an apostrophe needs the double quotes
q:'say "this"'    and the reverse
```

There are no escapes: the first matching close ends the term.

**Arabic and English are searched together.** Every record in scope is tried
against both its `ar` and its `en`, so `q:2:"prayer"` and `q:2:"الصلاة"` both
work without saying which language you mean.

Results are ordinary records, in the order they appear in the text. A search
that matches nothing is an empty `results`, not an error.

`~` rather than `-` marks a search scope, which keeps it apart from the `1-5`
of an ordinary selector — `Q:1:3-5` returns three ayat, `Q:1:3~5:"x"` searches
them.

**Arabic is matched with the marks folded away.** The Quran text is fully
diacritized, so a typed `الحمد` shares no substring with the stored
`ٱلْحَمْدُ`. For comparison only, QQL drops harakat, sukun, the superscript
alef and the Quranic annotation marks, folds the alef seats (`أ إ آ ٱ` → `ا`),
`ى` → `ي` and `ة` → `ه`, and lowercases ASCII. Records come back with their
text exactly as stored — nothing rewrites scripture.

Matching is plain substring, not word or stem matching: `"mercy"` does not
find *Merciful*, and `"pray"` does find *prayer*. It covers `ar` and `en` but
not metadata, so `"Al-Fatihah"` finds nothing — Surah names are not part of
the verse text.

The one exception is a word's *other* spellings. A single word is also tried
under the spellings the [alias table](#spelling-in-english) gives it, and
those must stand as whole words — `q:"eisa"` looks for `isa` as well, but not
inside *Isaac* or *disaster*. One trailing `s` is allowed, because folding
turns *Abraham's* into `abrahams`. The word you typed is still a substring.

The scan is linear over the scope — at 6236 ayat a whole-Quran search takes
about 100 ms cold, including reading every chapter file, and there is no index
to fall out of step with the text. Sources that declare no book-wide axis
(a custom source without a `flat` block) refuse an unscoped search with
`QQL_UNSUPPORTED` rather than quietly searching less than you asked for.

### Three ways to search

| Form | Matches | Order | Needs |
| --- | --- | --- | --- |
| `"term"` `'term'` | folded substring | positional | nothing |
| `?"term"` `?'term'` | words, stemmed, BM25 | **ranked** | `fulltext` feature + index |
| `*"term"` `*'term'` | vector similarity | **ranked** | `vector` feature + index |

The spelling picks the engine, so a build flag never changes what a query
means. Both optional engines are refused with `QQL_UNSUPPORTED` when their
feature or index is missing, rather than quietly falling back to substring
matching.

### Full-text search — optional

Backed by [tantivy](https://github.com/quickwit-oss/tantivy). A large
dependency, so it is a feature, **off by default**:

```toml
qql = { git = "https://github.com/mazhar266/QQ-Lang", tag = "v4.1.0", features = ["fulltext"] }
```

```bash
# 1. build the indexes once (every source, or just one)
cargo run --features fulltext --bin qql-index
cargo run --features fulltext --bin qql-index -- --source Q

# 2. query — mind the outer single quotes, or the shell eats the inner ones
cargo run --features fulltext -- 'q:?"mercy"~5'
cargo run --features fulltext -- "q:?'mercy'~5"
```

What it brings over `"term"`:

```text
q:1:"mercy"        0 hits  — "Merciful" does not contain "mercy"
q:1:?"mercy"       2 hits  — stemming reaches it
```

The term carries tantivy's own query syntax, so boolean and phrase queries
work. A phrase needs inner quotes, which is what the other delimiter is for:

```text
q:?"mercy OR forgiveness"~3
q:?"prayer AND charity"~3
q:?"prayer -charity"~3
q:?'"straight path"'~3        phrase
```

Every field is indexed **folded**, English included, and the query is folded
the same way. Arabic needs it because the corpus is fully diacritized and an
index over raw tokens would only match a query reproducing every mark. English
needs it for the apostrophes — see below. English then also runs through
tantivy's `en_stem` tokenizer, which is what makes `?"mercy"` find *merciful*.

Terms are combined with **OR**, and BM25 ranks whatever matches most. An
explicit `AND` still means AND. Requiring every word by default meant a single
word the corpus spells differently answered a natural-sounding query with
nothing at all.

A **plain** term — no phrase, boolean or field syntax — also has its stopwords
dropped and its alternate spellings expanded before parsing. A term carrying
syntax is passed through untouched, so `?'"the straight path"'` still means
exactly what it says.

The indexes are committed, like the vector ones, so a checkout can search
without a build step. Rebuild after changing text — 21 indexed sources,
29 MB in all:

```bash
cargo run --features fulltext --bin qql-index
```

A rebuild replaces every segment file, since tantivy names them by UUID. That
is a large diff each time, and each rebuild adds its own copy to git history —
worth batching with other data changes rather than doing casually.

### Similarity search — optional

A `*` in front asks for *similar* rather than *contains*. It is a cargo feature,
**off by default**, so the core crate stays at two dependencies and needs no
extra assets:

```toml
qql = { git = "https://github.com/mazhar266/QQ-Lang", tag = "v4.1.0", features = ["vector"] }
```

```bash
python3 scripts/build-vectors.py --source Q    # generates sources/vectors/Q.qv
cargo run --features vector -- 'q:*"worship"~3'
```

```text
q:*"worship"~3      top 3 across the Quran   → 109:2, 109:3, 109:4
q:1:*"worship"      within Surah 1           → 1:5
q:1:3~5:*"help"     within ayat 3–5          → 1:5
q:1:*"حمد"          undiacritized, no article → 1:2
```

Hits are ordinary records with two fields added: `score` and `"ranked": true`.
**This is the one place in QQL where output is ordered by relevance rather
than position** — everything else preserves the order written. `~N` caps the
result count (default 20), and weak matches are dropped, so a search can
return fewer than the cap or nothing at all.

Without the feature, or without an index for that source, a `*"…"` query is
**refused** with `QQL_UNSUPPORTED` naming the fix. It never silently falls
back to substring matching — answering a different question than the one asked
is worse than saying no.

#### What it actually does

The shipped embedder is a **signed hash projection of folded tokens** — whole
words plus character trigrams, hashed onto 256 dimensions, normalized and
quantized to `int8`. No model, no weights, no asset beyond the index itself,
and query embedding is a handful of hashes rather than a transformer.

Tokens are **weighted, not counted**. Each is scaled by its inverse document
frequency, a whole word outweighs one of its own trigrams three to one, and
stopwords are zeroed. Without that, *is* weighed what *quran* weighed and a
five-letter word voted four times — once as itself, three times as its
trigrams:

```text
q:*"quran is easy"~6      unweighted        weighted
  1.                      36:2  wise Qur'an   74:10 not easy
  2.                      56:77 noble Qur'an  54:17 made the Qur'an easy ←
  3.                      50:1  honored       54:22 …
  …                       54:17 at rank 11    54:32, 54:40 at 4 and 5
```

The weights ride inside the `.qv` file, keyed by token hash rather than by
word, so there is still no vocabulary to ship — about 0.3 MB per source, since
tokens seen in a single document all share the maximum IDF and are left out.

That still makes it **fuzzy lexical matching, not semantic**. It tolerates
diacritics, prefixes and suffixes — which is worth a great deal for Arabic —
but it does not know that *charity* and *zakat* are related, and a nonsense
query returns weak noise rather than nothing. For a phrase describing an ayah
rather than quoting it, `?"…"` is the better tool.

Real semantic vectors are a build-time swap: emit an index with a different
embedder id and teach [src/vector.rs](src/vector.rs) to embed queries the same
way. The file format, the scan, the scoping and the result shape are all
unchanged by that — which is exactly how the weighted embedder above was added
alongside the original one rather than replacing it.

#### Cost

| | |
| --- | --- |
| Index size | ~5.1 MB Quran, ~17 MB the Bible, ~53 MB all 21 indexed sources (weights included) |
| Query | hash the term, then a flat `int8` dot-product scan |
| Structure | none — no ANN index, nothing to fall out of sync |

A scan covers one source, and the largest — the whole Bible, 31,104 verses —
is about eight million integer multiply-accumulates; an
approximate-nearest-neighbour index would add a large dependency and a second
artifact that can drift, to beat something already fast enough on a weak
core. Scoping narrows the scan further.

All 21 indexes are committed, so the feature works without a build step.
Rebuild after changing text:

```bash
python3 scripts/build-vectors.py              # all sources, 53 MB
python3 scripts/build-vectors.py --source Q   # just one
```

### Source codes

| Code | Collection | `primary` is | `CODE::N` |
| --- | --- | --- | --- |
| `Q` | Quran | Surah, 1–114 | 1–6236 |
| `B` | Sahih al-Bukhari | chapter (kitab) | 1–7563 |
| `M` | Sahih Muslim | chapter | 1–7563 |
| `AD` | Sunan Abi Dawud | chapter | 1–5274 |
| `T` | Jami' at-Tirmidhi | chapter | 1–3956 |
| `N` | Sunan an-Nasa'i | chapter | 1–5758 |
| `IM` | Sunan Ibn Majah | chapter | 1–4341 |
| `MA` | Muwatta Malik | chapter | 1–1858 |
| `DA` | Sunan ad-Darimi (Arabic only) | chapter | — |
| `RS` | Riyad as-Salihin | chapter | — |
| `BM` | Bulugh al-Maram | chapter | — |
| `AM` | Al-Adab Al-Mufrad | chapter | — |
| `MK` | Mishkat al-Masabih | chapter | — |
| `SM` | Ash-Shama'il Al-Muhammadiyah | chapter | — |
| `NW` | Al-Arba'in an-Nawawiyyah | chapter (one: 1) | 1–42 |
| `QD` | Forty Hadith Qudsi | chapter (one: 1) | 1–40 |
| `SW` | Forty Hadith of Shah Waliullah | chapter (one: 1) | 1–40 |
| `HM` | Hisnul Muslim (alias `HISN`) | chapter, 1–132 | 1–267 |
| `PS` | Psalms — the Zabur (aliases `ZABUR`, `PSALMS`) | psalm, 1–150 | 1–2527 |
| `BI` | Bible — 66 books, testament optional: `BI:OT:GEN:1:1`, `BI:GEN:1:1` | chapter, after a book | 1–31104 |
| `OT` | Old Testament — Genesis to Malachi: `OT:GEN:1:1` | chapter, after a book | 1–23145 |
| `NT` | New Testament — Matthew to Revelation: `NT:JHN:3:16` | chapter, after a book | 1–7959 |
| `TO` | Torah — the Tawrat, Jewish edition: `TO:GEN:1:1` | chapter, after a book | 1–5853 |

`qql --sources` prints the registered codes.

The three forties are undivided books, so their whole text is chapter 1 and
`NW::13` and `NW:1:13` name the same hadith.

**Psalms** is the Jewish edition, in the Hebrew text's numbering. Each verse
carries five public-domain texts:

- the Hebrew Masoretic text as `he`;
- JPS 1917, the Jewish English translation, as `en`;
- the Smith & Van Dyck Arabic (1865) as `ar`;
- the King James Version and the World English Bible as `en_kjv` and `en_web`.

Search covers `ar` and `en`; the others are returned but not searched.

Hebrew numbering counts a psalm's title as a verse of its own: `PS:3:1` is
"A Psalm of David, when he fled from Absalom his son" and the prayer starts
at `PS:3:2`. So the book has 2527 verses where English Bibles have 2461. Every
record's `english_verses` gives the number(s) English Bibles use (`[1]` for
both of those). Where a title shares its verse with the first line, as in
Psalm 23, the two numberings agree.

The Bible's own Psalms, numbered the English way, are `OT:PSA`. Every `PS`
record's `bible_ref` is a query for the same passage there: `PS:3:2` carries
`"bible_ref": "OT:PSA:3:1"`.

### The Bible and the Torah

A Bible verse needs a book as well as a chapter and verse, so these sources
take a **path** — the book's name between the code and the numbers:

```text
OT:GEN:1:1          Genesis 1:1
NT:JHN:3:16         John 3:16
BI:NT:JHN:3:16      the same verse, through the whole Bible
BI:JHN:3:16         the testament may be left out
NT:1JN:4:8          a name may start with digits
OT:GEN              all of Genesis
BI:OT               the whole Old Testament
OT:GEN::32          the 32nd verse of Genesis, counted through the book (2:1)
NT::1               the New Testament's first verse
TO:GEN:32:1         the Jewish Torah, Hebrew numbering
TO:BERESHIT:1:1     Hebrew book names, in the Torah
```

A book is named by its USFM code (`GEN`, `1SA`, `JHN`), eBible's variant
(`JOH`, `MAR`, `SOL`), or its English name without spaces (`GENESIS`,
`1SAMUEL`, `SONGOFSONGS`). The five books of the Torah also answer to their
Hebrew names: `BERESHIT`, `SHEMOT`, `VAYIKRA`, `BAMIDBAR`, `DEVARIM`. Names
are case-insensitive, and the parser knows none of them — `Q:GEN` fails exactly
as it always did.

The path carries forward with its source, like the code itself:
`OT:GEN:1:1;3:1` is Genesis 1:1 and 3:1. A name at the start of a reference
is still a source code, though, so `OT:GEN:1:1;EXO:3:14` asks for a source
called `EXO` — write `OT:EXO:3:14`.

**Two editions, on purpose.** `BI`, `OT` and `NT` are the Christian Bible in
English verse numbering:

- `ar` is the Smith & Van Dyck Arabic;
- `en` is the King James Version;
- `en_web` is the World English Bible;
- the **original language**: `he`, the Hebrew Masoretic text (with its
  Aramaic passages), on every Old Testament verse; `grc`, the Greek Textus
  Receptus the KJV was translated from, on every New Testament verse.

The Hebrew is numbered differently from English Bibles, so it is placed on
the English verses: `OT:GEN:31:55` carries what the Hebrew numbers 32:1.

`TO` is the **Jewish Torah**: the Hebrew Masoretic text as `he`, JPS 1917 as
`en`, and the Van Dyck, KJV and WEB re-cut to the Hebrew verses as `ar`,
`en_kjv` and `en_web`. The Hebrew and English numberings differ in ten places
in the Torah, where a verse or two moves across a chapter boundary, so every
`TO` record's `bible_ref` names the same verse in `OT`:

```text
TO:GEN:32:1    "And early in the morning Laban rose up…"    bible_ref OT:GEN:31:55
TO:EXO:7:26    "…Let My people go, that they may serve Me." bible_ref OT:EXO:8:1
TO:NUM:25:19   "And it came to pass after the plague,"      bible_ref OT:NUM:26:1
```

The Psalms work the same way: `PS` is the Jewish edition, `OT:PSA` the
Christian one. Search covers `ar` and `en` in every edition; the Hebrew and
the second English texts are returned, not searched. Ranked search reads one
index for the whole Bible, which `OT` and `NT` see as windows, so
`OT:?"covenant"` and `BI:OT:?"covenant"` rank exactly alike.

A dash in the last column means the collection has **no citation numbering QQL
can source**: the data carries a per-chapter position and nothing else, and
the sequential position within the book is not the number these works are
cited by. Publishing it as one would answer real citations with the wrong
hadith, so `DA::1` is refused with `QQL_UNSUPPORTED`, and so is an unscoped
exact search `DA:"..."`, which would have to walk that axis. Scoped search
(`DA:1:"..."`) and the indexed engines (`DA:?"..."`, `DA:*"..."`) are
unaffected — they never needed a book-wide number.

### Numbering

There are two ways to address an item, and every source supports both.

**Within a chapter** — `SOURCE:primary:n` counts from 1 inside `primary`:

- **Quran** — `Q:2:255` is Surah 2, ayah 255.
- **Hadith** — `B:1:1` is the first hadith of chapter 1 (Kitab Bad' al-Wahy), matching the upstream per-chapter files.
- **Hisnul Muslim** — `HM:27:1` is the first supplication of chapter 27.
- **Psalms** — `PS:23:1` is Psalm 23, verse 1.
- **The Bible and the Torah** — `NT:JHN:3:16` is John 3:16; the book comes first.

**Across the whole book** — `SOURCE::n` skips the chapter and uses the
**canonical citation numbers**: 'Abd al-Baqi's numbering for Bukhari
(1–7563), Dar-us-Salam's for Muslim, the sunnah.com reference numbers for the
rest. `B::6403` returns what the world cites as Bukhari 6403.

```text
B::6403       the "la ilaha illallah a hundred times" hadith
B::7563       the famous closing hadith of Sahih al-Bukhari
Q::100        the 100th ayah of the mushaf (Surah 2, ayah 93)
HM::75        the 75th supplication of Hisnul Muslim
PS::2527      the last verse of the Psalms (Psalm 150, verse 6)
BI::31104     the last verse of the Bible (Revelation 22:21)
OT:GEN::32    the 32nd verse of Genesis (2:1) — `::` counts through a book too
B::1-10,255   ranges and lists work the same
```

For hadith this resolves through small committed maps
(`sources/canonical/*.json`, built by `scripts/build-canonical.py` from the
public-domain [fawazahmed0/hadith-api](https://github.com/fawazahmed0/hadith-api)
dataset and validated against the local text). The canonical space has holes —
front matter such as Muslim's Muqaddima owns canonical 1–92, and lettered
variants like 1771.5 are not integers. Asking for such a number alone is an
error; a range simply skips them.

Records from the flat form carry `"numbering": "book"`, so a single query
mixing both forms stays unambiguous, and they still report their chapter.

### Examples

```text
Q:1                    all of Surah 1
Q:2:255                Ayat al-Kursi
Q:2:1-5                ayat 1 through 5
Q:2:1-5,10,20-25,255   mixed ranges and singles
Q:1;Q:2:255;Q:112;     three references
B:1:1-10               Bukhari, chapter 1, hadith 1–10
HM:27                  Hisnul Muslim, all of chapter 27 (morning and evening remembrance)
HM:27:1-3              the first three supplications of that chapter
PS:23                  Psalm 23, all six verses
PS:3:1                 Psalm 3's title, a verse of its own in Hebrew numbering
PS:51:3-6;Q:2:255      the Psalms mix with everything else
ps:"Selah"             every verse of the Psalms that says it — 71
NT:JHN:3:16            John 3:16
OT:GEN:1:1-5;Q:1       the Bible mixes with everything else
NT:JHN:3:"born again"  search one chapter of John
BI:OT:?"covenant"~5    ranked search over the Old Testament
TO:GEN:32:1            the Jewish Torah — OT:GEN:31:55 in English numbering
B::100                 Bukhari hadith 100, traditional book-wide numbering
Q::1-7;B::1;           flat form mixes freely with the rest
1                      no source code — the whole of Surah 1
1,2:255                all of Surah 1, then Surah 2 ayah 255
q:1:2,3,2:3,4-6        two groups under one source
b:1:1;3                the source carries forward — Bukhari twice
b:1:1;q:3              …until another code switches it
q:1:"الحمد"            search Surah 1 in Arabic
q:2:"prayer"           search Surah 2 in English
q:1:3~5:"You"          search ayat 3–5
'mercy'                either quote works
```

### Ordering and duplicates

Query order is preserved — `Q:2:255,1-3` returns 255, 1, 2, 3. Nothing is sorted for you.

Duplicates are removed **within** a single reference (`Q:2:1-5,3,4` → 1, 2, 3, 4, 5) but **across** references they are kept (`Q:2:255;Q:2:255;` returns two items).

## Build

```bash
cargo build --release
cargo test
```

The crate builds as `rlib`, `cdylib`, and `staticlib`, producing `libqql.so` / `qql.dll` / `libqql.dylib` plus `libqql.a`, alongside the `qql` binary.

Lints and the C ABI check are part of the build contract:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
./scripts/c-smoke.sh                    # C header + link check
cargo +nightly miri test --test ffi     # after touching src/ffi.rs
cargo +nightly fuzz run parse           # the parser must never panic
```

## Releases

Tagged releases ship self-contained bundles per platform — the `qql` CLI,
`qql-index`, the C libraries and header, and all data and search indexes.
Extract and run; no build, no setup:

```bash
tar xzf qql-v4.1.0-x86_64-linux.tar.gz
cd qql-v4.1.0-x86_64-linux
./qql 'Q:2:255'
./qql 'q:1:?"mercy"~5'        # both search engines included
```

Maintainers cut one by pushing a tag: `git tag v4.1.0 && git push origin v4.1.0`. The tag alone decides the version — it is stamped into the build, so no `Cargo.toml` bump commit is needed first.

## CLI

```bash
cargo run --bin qql -- "Q:2:255"
./target/release/qql "Q:2:1-5,255;Q:1;"
./target/release/qql --data ./sources "B:1:1-3"
./target/release/qql --parse "Q:2:1-5,255;Q:1;"   # parse only, no data access
./target/release/qql --compact "Q:1"
./target/release/qql --sources
```

| Option | Meaning |
| --- | --- |
| `--data <DIR>` | Data directory (default `./sources`) |
| `--source <F>` | Register sources from a manifest, relative to `--data`. Repeatable. |
| `--parse` | Print the parsed query instead of resolving it |
| `--compact` | Compact JSON instead of pretty-printed |
| `--sources` | List registered source codes |

Output is pretty-printed JSON, exit code `0` on success and `1` on error — the JSON goes to stdout either way, so it stays pipeable into `jq`. The library itself returns compact JSON.

## Rust usage

```toml
[dependencies]
qql = { git = "https://github.com/mazhar266/QQ-Lang", tag = "v4.1.0" }
```

QQL is not on crates.io — the `qql` crate there is an unrelated project, so
depend on the repository by tag.

```rust
use qql::{Context, Error};

fn main() -> Result<(), Error> {
    let mut ctx = Context::new("./sources");

    for record in ctx.execute("Q:2:1-5,255;Q:1;")? {
        println!("{} — {}", record.collection, record.ar);
    }

    Ok(())
}
```

Parsing alone needs no data directory and touches no filesystem:

```rust
let query = qql::parse("Q:2:1-5,255")?;
assert_eq!(query.references.len(), 1);
```

`Context::execute_json` returns a `String` instead, and never fails — errors are serialized into the JSON. `execute_value` is the same thing before serialization. Those are the total functions the FFI layer will wrap.

## Result format

```json
{
  "ok": true,
  "query": "Q:2:255",
  "results": [
    {
      "source": "Q",
      "collection": "Quran",
      "surah": 2,
      "surah_name_ar": "البقرة",
      "surah_name_en": "Al-Baqarah",
      "ayah": 255,
      "ar": "...",
      "emlaei": "...",
      "en": "..."
    }
  ]
}
```

Every record carries `source`, `collection`, `ar`, and `en`. Other fields are source-specific — Hadith records use `book`/`chapter`/`number` instead of `surah`/`ayah`, Psalms records use `psalm`/`verse` in Hebrew numbering and add `english_verses`, `bible_ref`, `he`, `en_kjv` and `en_web` beside the JPS 1917 `en`. Bible records carry `testament`, `book` (the USFM code), `book_name_en`, `book_name_ar`, `chapter`, `verse`, `en_web`, and `he` (Old Testament) or `grc` (New Testament); Torah records drop `testament` and add `book_name_he`, `he`, `en_kjv` and `bible_ref`.

### Two Arabic spellings

The Quran also carries `emlaei`, the simplified spelling, beside the Uthmani `ar`.
They differ in the consonantal skeleton and not merely in marks — `ٱلصَّلَوٰةَ` against
`الصلاة`, `ٱلْكِتَٰب` against `الكتاب` — so folding cannot turn one into the other, and a
search typed in modern orthography would silently return nothing for some of the most
common words in the text. All three engines search both spellings and the English:

```text
q:"الكتاب"          162 ayat   (stored as ٱلْكِتَٰب)
q:?"الصلاة"~20      ranked     (stored as ٱلصَّلَوٰةَ)
q:*"الحياة"~5       ranked     (stored as ٱلْحَيَوٰةِ)
```

`ar` is unchanged — the mushaf text is what a record returns and what `--data` ships.
Sources with a single spelling leave `emlaei` empty, and it is then omitted from the
JSON, so hadith and Hisnul Muslim records are byte-identical to before.

One caveat about the supplied text: it keeps a few classical spellings, most visibly
`السموات` rather than `السماوات`. Search that word the way the data writes it.

### Spelling in English

Transliteration is not consistent, in this corpus or in anyone's head, so search
normalizes two kinds of variation.

**Apostrophes are folded away.** Every engine here tokenizes on non-alphanumerics, so
`Qur'an` used to index as `qur` + `an` and a search for `quran` matched neither half.
The corpus is not even self-consistent — it writes `qur'an` 199 times and `qur’an` 50,
`rak'ahs` beside `rak’ahs`, and `` `Asr `` with a backtick. All of `'` `‘` `’` `` ` ``
`ʼ` `ʻ` `ʾ` `ʿ` now fold to nothing, collapsing them onto one token.

**A short alias table** maps the spellings that differ by more than punctuation:

```text
q:"koran"          121 ayat   (the text writes Qur'an)
q:"ibrahim"         72 ayat   (the translation writes Abraham)
q:"eisa"            33 ayat   (the translation writes Jesus)
q:?"quran is easy"  54:17, 54:22, 54:40
```

It is about thirty entries — `quran`/`koran`, `muhammad`/`mohammed`, `salah`/`salat`,
`zakah`/`zakat`, and the prophets' Arabic names against the English ones the
translations print. It is deliberately *not* general: expansion happens at query time,
so the table can grow without rebuilding an index, and words that would collide with
ordinary English (`lut`/`lot`, `ayyub`/`job`) are left out on purpose.

**A prophet's name expands one way**, from Arabic to English. `"musa"` finds *Moses*,
but `"moses"` does not look for `musa`: narrators carry the prophets' names, and a
reader who types the English one means the prophet alone. When it did expand,
`ma:"john"` returned 1577 hadith of the Muwatta — nearly every one opens *Yahya related
to me from Malik* — and `ad:"david"` 757, nearly all for *Abu Dawud said*. The price is
a hadith translation that prints the Arabic name for the prophet himself: Tirmidhi
writes *'Eisa*, so `t:"jesus"` finds nothing there. Search `t:"eisa"`, which asks for
anyone of that name. All three engines read the same table, so this holds for `?"…"`
and `*"…"` as well.

Exact search matches a substituted spelling only as a whole word. As a substring,
`isa` sits inside *Isaac*, *Isaiah* and *disaster*, `musa` inside *Musayyab*, and `haj`
inside *tahajjud*. The cost is the occasional longer word that did belong: `"koran"`
no longer finds a record that says only *Quranic*. Type `"quran"` for that — your own
word still matches anywhere. The ranked engines compare whole tokens, so they never
had the problem.

Errors are still valid JSON, never a malformed string:

```json
{
  "ok": false,
  "error": {
    "code": "QQL_INVALID_RANGE",
    "message": "Range start cannot be greater than range end",
    "position": 7
  }
}
```

Error codes: `QQL_EMPTY_QUERY`, `QQL_INVALID_CHARACTER`, `QQL_EXPECTED_SOURCE`, `QQL_EXPECTED_COLON`, `QQL_EXPECTED_NUMBER`, `QQL_EXPECTED_TEXT`, `QQL_UNTERMINATED_TEXT`, `QQL_INVALID_RANGE`, `QQL_UNKNOWN_SOURCE`, `QQL_UNSUPPORTED`, `QQL_REFERENCE_NOT_FOUND`, `QQL_DATA_FILE_NOT_FOUND`, `QQL_INVALID_DATA_FILE`, `QQL_INTERNAL_ERROR`.

`position` is a byte offset into the query and is present only for errors that have one.

All input and output is UTF-8. Arabic passes through byte-for-byte — tashkeel, Quranic marks, and zero-width characters are never normalized, and invalid UTF-8 is rejected rather than lossily replaced.

## C / FFI usage

[include/qql.h](include/qql.h) is committed, so C consumers need no Rust toolchain. `scripts/c-smoke.sh` compiles [examples/c/basic.c](examples/c/basic.c) against it and links it to the real library — a signature that drifts from [src/ffi.rs](src/ffi.rs) fails there rather than at a user's link step.

```bash
cargo build --release
./scripts/c-smoke.sh            # builds, links, runs, and checks under ASan when available
```

```c
#include <stdio.h>
#include <qql.h>

int main(void) {
    qql_context_t *ctx = qql_context_create("./sources");
    if (!ctx) return 1;

    char *result = qql_context_execute(ctx, "Q:2:1-5,255;Q:1;");
    printf("%s\n", result);

    qql_free_string(result);
    qql_context_destroy(ctx);
    return 0;
}
```

```bash
cc example.c -Iinclude target/release/libqql.a -lpthread -ldl -lm -o example   # static
cc example.c -Iinclude -Ltarget/release -lqql -o example                        # shared
```

### Ownership

```text
qql_context_execute()  allocates the returned buffer
qql_free_string()      frees it
```

- Returned strings are NUL-terminated UTF-8.
- The **caller** owns the buffer once returned and must release it with `qql_free_string()` — never with the host language's allocator, and never with plain `free()`.
- `qql_version()` is the one exception: it returns a static string that must **not** be freed.
- `qql_context_t` is opaque; only the pointer crosses the boundary.
- Panics never cross the FFI boundary. Every entry point catches them and returns an error JSON string instead.
- `qql_context_execute` never returns null and never returns malformed JSON — a null context, a null query, and invalid UTF-8 all come back as `{"ok":false,...}`.
- Result strings stay valid after `qql_context_destroy`; they are independently owned.

### Dart

A `dart:ffi` binding lives in [bindings/dart/](bindings/dart/) — see its README for testing, bundling, and memory notes.

```bash
cargo build --release --features vector,fulltext   # features are baked into the .so
cd bindings/dart && dart pub get && dart test
```

```dart
final qql = Qql.open('sources', libraryPath: 'target/release/libqql.so');
try {
  for (final record in qql.execute('Q:2:1-5,255')) {
    print(record['ar']);
  }
} finally {
  qql.dispose();
}
```

### Thread safety

`Context::execute` takes `&mut self`, so the Rust compiler prevents concurrent use of one context. `Context` is `Send`, so moving one to another thread is fine, and separate contexts on separate threads are safe by construction.

That guarantee does not survive the FFI boundary — **do not use one `qql_context_t` from two threads at once.** `qql_execute()` uses a process-wide default context behind a mutex: thread-safe, but serialized. Prefer explicit contexts.

## Architecture

```text
query → lexer → parser → AST → validation → source resolver → repository → serializer → UTF-8 String
```

The layers stay independent:

```text
QQL parser knows syntax.
Source handlers know Islamic-book structure.
Repository knows storage.
FFI module knows the C ABI.
```

The parser only knows that a reference is `IDENT : INT ( : selectors )`. It has no idea that Surah numbers stop at 114 — that is the Quran resolver's job. So `Q:500:999` and `XYZ:1:2` both parse fine and are rejected later, by the resolver and the registry respectively.

`unsafe` exists in exactly one module, `src/ffi.rs`. The rest of the crate is `#![deny(unsafe_code)]`.

## Adding a source without writing Rust

Point QQL at your own JSON and give it a short code. Drop a `qql-sources.json` in the data directory and it is picked up automatically — no rebuild, and it works through the CLI, the C ABI, and Dart alike.

```json
[
  {
    "code": "X",
    "name": "My Collection",
    "aliases": ["MINE"],
    "path": "mydata/{primary}.json",
    "items": "lines",
    "ar": "arabic",
    "en": "translation",
    "primary_key": "chapter",
    "container_metadata": { "chapter_title": "title" },
    "metadata": { "note": "note" }
  }
]
```

```bash
qql --data ./mydata "X:1:2"
qql --source other-sources.json "X:1:2"    # extra manifest, repeatable
qql --data ./mydata --sources              # X now listed
```

| Key | Meaning |
| --- | --- |
| `code`, `name` | Short code and display name. Codes uppercase automatically. |
| `aliases` | Extra codes that select this source. |
| `path` | Data file, relative to the data directory. `{primary}` → one file per chapter; omit it for a single file. |
| `items` | Dotted path to the array of items. Empty means the file *is* the array. |
| `ar`, `en` | Dotted paths within an item, so `"english.text"` reaches nested fields. |
| `chapters` + `chapter_id` | For single-file books: the chapter array, and the field to match against the primary. |
| `item_id` | Match items by a field instead of by position. |
| `flat` | Enables `X::100`: `{ "path", "items", "item_id" }` pointing at the collection numbered straight through. Without it, a flat reference is an error. |
| `primary_key` | Names the primary in output (`surah`, `chapter`, …). Defaults to `primary`. |
| `metadata` | Extra output fields taken from the item: output key → dotted path. |
| `container_metadata` | Same, taken from the chapter or file. |

Everything else behaves as it does for built-in sources: query order preserved, duplicates dropped within a reference, ranges bounds-checked, errors as JSON.

From Rust, skip the file entirely:

```rust
let mut ctx = qql::Context::new("mydata");
ctx.register_spec(spec);                      // a SourceSpec
ctx.add_sources_from("other-sources.json")?;  // or a whole manifest
```

Sources are searched newest-first, so registering an existing code shadows it. The data-directory manifest loads on the first query, so call `ctx.load_manifest()?` first if you mean to override something it defines.

Reach for a real `impl Source` (below) when the data is too irregular for a mapping to express — the Hisnul Muslim resolver exists because its file has duplicate keys and a misspelled field.

## Adding a new source in Rust

Adding a collection requires **no** change to the lexer or parser.

If it is another book in the nine, it is a single line — `HadithCollection` is one `Source` implementation, instantiated once per collection:

```rust
Box::new(HadithCollection::new("MK", "Muwatta Malik", "malik"))
```

For a collection with a different structure, implement the trait:

```rust
use qql::{Error, Record, Reference, Repository, Source};

pub struct HisnulMuslim;

impl Source for HisnulMuslim {
    fn code(&self) -> &str { "HM" }
    fn name(&self) -> &str { "Hisnul Muslim" }

    fn resolve(
        &self,
        repo: &mut Repository,
        reference: &Reference,
        out: &mut Vec<Record>,
    ) -> Result<(), Error> {
        // Reject impossible numbers, load through `repo`, push records in
        // query order. `reference.expand(total)` handles ordering, dedup,
        // and bounds in one call.
    }
}
```

Then add the module to `src/sources/mod.rs`, one line to `Registry::with_defaults` in [src/registry.rs](src/registry.rs), and the data under `sources/`. Aliases (`TIRMIDHI` → `T`) come from the trait's `aliases()` method — no registry surgery required.

Semantic validation lives inside `resolve` rather than a separate `validate` method: every check a dry run would perform is the first thing `resolve` does anyway.

## Data

All text is committed directly under `sources/` — **no submodules**, no build
step, no network. A plain `git clone` is a complete working installation.

```text
sources/
  quran/chapters/{1..114}.json       Arabic + English, one file per Surah
  hadith/{book}/{chapter}.json       the hadith collections, per chapter
  hisnul-muslim/husn_en.json         all 132 chapters in one file
  psalms/{1..150}.json               Hebrew + JPS + Arabic + KJV + WEB, per psalm
  bible/books.json                   the 66 books: Arabic names, verses per chapter
  bible/{BOOK}/{chapter}.json        Arabic + KJV + WEB + Hebrew or Greek, English numbering
  torah/books.json                   the five books, in Hebrew numbering
  torah/{BOOK}/{chapter}.json        Hebrew + JPS + Arabic + KJV + WEB
  canonical/{CODE}.json              citation-number maps for B::N
  vectors/{CODE}.qv                  similarity indexes (vector feature)
  fulltext/{CODE}/                   tantivy indexes (fulltext feature)
```

The hadith and Hisnul Muslim data originate from
[hadith-json](https://github.com/AhmedBaset/hadith-json) (ISC) and
[Hisn-Muslim-Json](https://github.com/wafaaelmaandy/Hisn-Muslim-Json), carried
unmodified with `ATTRIBUTION.md` files alongside. Only the collections QQL
resolves are included.

### The Quran directory is generated, not vendored

`sources/quran/` is built by [scripts/build-quran.py](scripts/build-quran.py) and committed:

```bash
python3 scripts/build-quran.py --meta DIR            # fetch Tanzil, rebuild
python3 scripts/build-quran.py --meta DIR --check    # verify only, write nothing
```

The Arabic comes from **Tanzil's Uthmani text**; names, the English translation and
the per-ayah transliteration still come from the `quran-json-arabic` project,
fetched only when rebuilding. The simplified `emlaei` spelling is read from the
committed `sources/hafs_smart_v8.json` — small enough to carry, and pairing scripture
with the wrong ayah is not something a network fetch should be able to do quietly.

The split exists because that package spells three marks with codepoints that mean
something else in Unicode — `U+0657 INVERTED DAMMA` for an open fathatan, `U+065E` for
a dammatan, `U+0656` for a kasratan. A font that follows Unicode draws them literally,
so 2:286's `إِصْرًا` gains a damma above the reh and reads *isru* rather than *isran*.
Tanzil uses the standard marks and carries the pause, sajdah and silence signs the
package omits.

It also refuses to write unless every ayah's Emlaei spelling lines up with its Uthmani
one. Since the two rasms disagree only about alef, waw and ya, a skeleton with those
letters removed is nearly identical for a correctly paired ayah: 1.3% differ, against
100% for an off-by-one pairing, so the 5% limit catches a shifted file with room to
spare.

The generator refuses to write unless the two texts agree: same ayah counts per surah,
no misused codepoint in the output, the basmalah stripped from exactly the 112 surahs
that carry it, and the consonant skeletons matching except for the handful of ayat
where the two disagree about hamza spelling (`ئ` versus `ي` plus a combining hamza).

Files are read on first use and cached in the `Context` until it drops — `Q:2:255` loads roughly 50 KB, not the whole mushaf. There is no eviction policy.

### The Psalms directory is generated too

`sources/psalms/` is built by [scripts/build-psalms.py](scripts/build-psalms.py)
from eBible.org's USFM exports of the Hebrew (`hbo`), JPS 1917 (`engjps`), the
Van Dyck Arabic (`arb-vd`), the KJV (`eng-kjv2006`) and the WEB (`eng-web`),
unzipped into one directory:

```bash
python3 scripts/build-psalms.py --from DIR            # rebuild
python3 scripts/build-psalms.py --from DIR --check    # verify only, write nothing
```

The Hebrew defines the verses; the four translations arrive in English
numbering and are re-cut to fit them:

- JPS carries the Hebrew verse numbers inline and is divided exactly there.
  Its markers give the English-to-Hebrew map the others follow.
- Van Dyck, the KJV and the WEB keep each psalm's title apart from verse 1,
  and are divided at the title.

The script refuses to write unless every one of the 2527 Hebrew verses is
filled. The only exceptions are the second verse of the four two-verse titles,
which only JPS can divide. The words *Selah* and *David* must also fall in the
same verses in all five texts: they agree in all 71 and all 86–88 verses.
[sources/psalms/ATTRIBUTION.md](sources/psalms/ATTRIBUTION.md) lists exactly
what the build changes.

### So are the Bible and the Torah

```bash
python3 scripts/build-bible.py --from DIR    # sources/bible/  (arb-vd, eng-kjv2006, eng-web, hbo, engjps, grctr)
python3 scripts/build-torah.py --from DIR    # sources/torah/  (hbo, engjps, arb-vd, eng-kjv2006, eng-web)
```

All three builds read eBible.org's USFM through one shared converter,
[scripts/usfm.py](scripts/usfm.py). Measured against eBible's own plain-text
export, it reproduces the Van Dyck and the KJV word for word, all 31,104 and
31,102 verses.

- **`build-bible.py`** refuses to write unless the three texts differ in
  exactly the known places:
  - Van Dyck numbers two closing greetings as verses of their own (1 Timothy
    6:22, 3 John 1:15);
  - the WEB omits four verses and places Romans' doxology after 14:23.

  *Moses*, *David*, *Jesus* and *Abraham* must also fall in the same verses
  across the Arabic and both English texts. The Hebrew is placed on the
  English verses through [scripts/hebrew.py](scripts/hebrew.py), the same
  map the Torah and Psalms builds use. Every Hebrew verse must land on exactly
  one English verse, and both originals must agree with the KJV on where the
  anchor names fall.
- **`build-torah.py`** re-cuts the translations to the Hebrew verses, as the
  Psalms build does. JPS is divided at its inline Hebrew numbers, except in
  the Ten Commandments, where JPS follows a different accent tradition from
  the Hebrew text. The other three are divided at whole verses, with one
  checked cut inside Numbers 26:1. *Moses*, *Aaron*, *Pharaoh* and *Jacob*
  must fall in the same Hebrew verses in all five texts.

[sources/bible/ATTRIBUTION.md](sources/bible/ATTRIBUTION.md) and
[sources/torah/ATTRIBUTION.md](sources/torah/ATTRIBUTION.md) list exactly what
each build changes.

Licensing sits next to the data: `sources/quran/TANZIL-LICENSE.txt` for the
Quran text, and the `ATTRIBUTION.md` files in `sources/hadith/`,
`sources/hisnul-muslim/`, `sources/psalms/`, `sources/bible/` and
`sources/torah/`.

Upstream data is taken as authoritative and is never rewritten, but it is not uniform. The Hisnul Muslim file in particular carries a UTF-8 BOM, stores its chapters out of numerical order, repeats a key in two entries, and misspells one field name. Those are absorbed where they belong — the BOM in [src/repo.rs](src/repo.rs) since it is a storage concern, the rest in [src/sources/hisnul.rs](src/sources/hisnul.rs) — rather than by patching the data.

## Contributing

Contributions are welcome — see [CONTRIBUTING.md](CONTRIBUTING.md) for build, test, and style requirements.

## License

GNU General Public License v3.0 or later — see [LICENSE.md](LICENSE.md).
