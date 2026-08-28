# Attribution

The chapter files in this directory come from the **hadith-json** project —
https://github.com/AhmedBaset/hadith-json — (ISC license), which compiled them
from sunnah.com. Only the collections QQL resolves are carried, unmodified,
from the project's `db/by_chapter` layout:

| Directory | Upstream path |
| --- | --- |
| `bukhari`, `muslim`, `abudawud`, `tirmidhi`, `nasai`, `ibnmajah`, `malik`, `darimi` | `the_9_books/` |
| `riyad_assalihin`, `bulugh_almaram`, `aladab_almufrad`, `mishkat_almasabih`, `shamail_muhammadiyah` | `other_books/` |
| `nawawi40`, `qudsi40`, `shahwaliullah40` | `forties/` |

The three forties ship upstream as a single `all.json`. Since QQL addresses
chapters by number, that file is carried here as `1.json` — its only edit is
the name.

Musnad Ahmad ibn Hanbal is deliberately not carried: upstream has 8 of its
musnads and 1374 of roughly 27,000 hadiths, which is too incomplete to publish
under a code that implies the whole collection.
