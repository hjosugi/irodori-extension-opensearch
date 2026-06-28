# Native Source

There is no existing desktop adapter source for this connector yet.



This directory is a migration staging area for `irodori.opensearch`. The active native
ABI shim lives in `src/lib.rs`; engine-specific connect/query/metadata behavior
should move here as the connector runtime contract is wired into the desktop app.

Engine status from `knowledge/engines.json`: `recognized_no_connector`.
