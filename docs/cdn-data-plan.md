# Plan for on-demand data and `qql --preload`

Proposed for a future release. A small QQL install should be able to answer its first query with a persistent download cache. On an empty cache, `qql 'Q:1:1'` needs one Quran chapter. A second query for another ayah in that surah should work offline. A later vector search adds the Quran vector index and any chapters needed to return its hits. The same files can be downloaded ahead of time with `--preload`.

## What each command downloads

| Command | Files selected |
| --- | --- |
| `qql 'Q:1:1'` | `quran/chapters/1.json` |
| `qql 'Q:1:"mercy"'` | The same chapter. Exact search scans the text. |
| `qql 'Q:*"mercy"~3'` | `vectors/Q.qv`, then the chapter files containing the returned ayat |
| `qql 'Q:?"mercy"~3'` | The complete `fulltext/Q/` index, then the chapter files containing the returned ayat |
| `qql --preload all` | Every published source’s normal data, vector index and full-text index, counting shared files once |
| `qql --preload 'q, b*, b?'` | All 114 Quran chapter files, `vectors/B.qv` and the complete `fulltext/B/` index |
| `qql --preload q-full` | All 114 Quran chapters, `vectors/Q.qv` and the complete `fulltext/Q/` index |

“Normal data” includes everything the ordinary resolver reads for that source. For Bukhari this includes the chapter files and `canonical/B.json`, which maps a reference such as `B::6403` to its chapter and item. An ordinary lookup fetches the map only when the resolver needs it. Ship the applicable text licenses and attribution notices with the client so they are available from the first lookup.

### Preload selectors

| Selector | Meaning |
| --- | --- |
| `CODE` | All normal data for one source |
| `CODE*` | Its vector index only |
| `CODE?` | Its full-text index only |
| `CODE-full` | Its normal data and every index published for that source |
| `all` | Every runtime asset in this data release |

Accept case-insensitive codes and a comma-separated list with optional spaces. Quote the whole list in a shell because `*` and `?` can expand before QQL sees them. Accept the equivalent split arguments `qql --preload q, 'b*', 'b?'` as well. Parse these selectors in the CLI. The existing query grammar already distinguishes exact, vector and full-text searches.

Validate the whole list before downloading. Report an unknown code, empty selector or explicitly requested index the release does not carry. Expand overlapping selections to unique files. Repeating a completed preload should transfer zero bytes. Preload can stage a published index even if this executable lacks that search feature. Running the search still requires the feature.

After `qql --preload 'q, b*, b?'`, ordinary Quran lookups work offline. A Bukhari search can still need chapter data to display a hit. Use `b-full` to prepare both Bukhari search engines and all their results for offline use.

## Release files and cache

1. **Publish a versioned inventory.** Generate it from the runtime assets staged by the release build. Record each file’s relative path, byte length, SHA-256 digest, source membership and role (normal, vector or full-text). Include the base catalogs and numbering maps each source reads. Group every Tantivy index’s metadata and segment files into one installable directory. Omit transient lock files and build inputs. Use the existing full bundle as the inventory cross-check. [Release packaging](../scripts/package.sh)
2. **Pin the data to the reader.** Give each dataset an immutable CDN prefix and embed its inventory in the matching thin CLI build. Publish and verify the objects before distributing that binary. An older binary can then retrieve chapters and compatible indexes from the same release on separate days. The CDN serves static files. Search terms and full query strings stay on the device.
3. **Keep a persistent per-user cache.** Under the platform cache directory, use `qql/<dataset-id>/` and preserve the relative paths used by the resolvers. Both preload and automatic fetching use this directory. Download a missing file to a temporary sibling, verify its size and digest, then rename it into place. For Tantivy, stage and verify the entire index before publishing the directory. Coordinate writers per file or index and recheck after acquiring the lock. An interrupted run can reuse completed downloads when retried.
4. **Keep local installs usable.** Retain the self-contained bundle and explicit `--data DIR` as local data sources. An ordinary CLI launch uses the local distribution when present and the managed cache for a thin install. Preload targets the managed cache. Keep explicit Rust, C and Dart data-directory contexts local by default. A local custom source, including one that shadows a built-in code, continues to use its configured files.

## Implementation order

1. **Build the inventory and thin artifact.** Start with Q and B against a local HTTP fixture. Expand to all shipped sources before publishing. Map `OT` and `NT` to their respective normal data and the shared `BI` search indexes. Keep the Jewish Torah’s `TO` assets separate. Count a shared object once when expanding `all`.
2. **Fetch on the existing read path.** Give the repository a way to ensure a release file is present before its JSON and binary reads. Add a directory-level equivalent where full-text search opens its index. Let the resolver request the paths, including any catalogs needed to scope a search. This covers shorthand, multi-source queries, ranges, canonical numbering and ranked hits without a second query parser. Parsing, listing sources and rejecting a disabled search engine should issue zero CDN requests. [Repository](../src/repo.rs) · [Query execution](../src/context.rs) · [Full-text reader](../src/fulltext.rs)
3. **Add the CLI command.** Expand selectors using the same inventory and call the same verified downloader. Exit successfully once all selected assets are usable. On a network, integrity or disk failure, identify the failed asset, exit unsuccessfully and retain completed files. Send progress to stderr so normal query output remains pipeable. Preserve the distinction between a failed download and a reference that does not exist in the source. [CLI](../src/bin/qql.rs)
4. **Verify before release.** Run the checks below against a request-logging local HTTP server and a fresh cache. Then smoke-test the thin binary against the published release and the full bundle with networking disabled.

## Acceptance checks

- Run each query in the table from an empty cache. Assert the exact requested file set and compare the result with the local bundle. Repeat `Q:1:1` in a new process with the server stopped. A later `Q:2:1` fetches only chapter 2 when the server is available again.
- Run all three preload examples. Check the resulting file sets, including the absence of Bukhari text after `'q, b*, b?'`. Repeat with overlapping selectors and a warm cache. An invalid selector at the end of a list must fail before the first download.
- Disconnect after `q-full` and run ordinary, exact, vector and full-text Quran queries. After `all`, run the existing bundle smoke queries offline, including a canonical Bukhari number and a search that uses the shared Bible index.
- Interrupt a transfer, serve a truncated file and start two processes requesting the same Tantivy index. Readers should only see verified, complete assets. Restore the server and verify that retry finishes the install. Run the local-directory and custom-source tests with networking disabled.
