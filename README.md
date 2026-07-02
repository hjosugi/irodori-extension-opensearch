# OpenSearch Connector

Adds OpenSearch index, search, mapping, aggregation, and bulk workflows as an installable connector extension.

This connector is listed in the public Irodori extension marketplace.

## Connector

- Extension ID: `irodori.opensearch`
- Engine ID: `openSearch`
- Wire: `search`
- Default port: `9200`
- Native ABI: `irodori.connector.native.v1`
- Driver linked: `true`

No desktop adapter source exists yet; this package starts from the refactored ABI shim and connector metadata.

Connector metadata lives in `connector.config.json` and `irodori.extension.json`.
The Rust code keeps native ABI exports in `src/lib.rs`, shared buffer/JSON helpers in `src/abi.rs`, and OpenSearch behavior in `src/driver.rs`.

## Connection Metadata

- Endpoint modes: `hostPort`, `connectionString`
- Transport modes: `direct`, `sshTunnel`, `socks5Proxy`, `httpConnectProxy`, `proxyChain`
- TLS supported: `true`
- Custom driver options: `true`

| Auth method | Label | Secret purposes |
|---|---|---|
| `none` | No authentication | none |
| `connectionString` | Connection string / DSN | none |
| `basic` | Basic authentication | `password` |
| `apiKey` | API key | `token` |
| `bearerToken` | Bearer token | `token` |
| `oauth2` | OAuth 2.0 | `token` |
| `clientCertificate` | Client certificate / mTLS | `privateKey`, `privateKeyPassphrase` |
| `awsSigV4` | AWS SigV4 | `token` |
| `customDriverOptions` | Custom driver options | `password`, `token`, `privateKey`, `privateKeyPassphrase` |

## Experience Metadata

- Domains: `search`, `vector`
- Result views: `searchHits`, `facets`, `json`, `table`, `vectorNeighbors`
- Inspired by: `OpenSearch Dashboards Discover`, `OpenSearch aggregations`, `OpenSearch hybrid search`, `OpenSearch k-NN search`

| Workflow | Result view | Templates |
|---|---|---|
| Discover documents | searchHits | search-query-string |
| Facet breakdown | facets | search-facets |
| Hybrid search | searchHits | search-hybrid |
| Similarity search | vectorNeighbors | vector-similarity |
| Filtered ANN search | vectorNeighbors | vector-filtered |
| Collection or index health | table | vector-health |

| Template | Label | Language | Result view |
|---|---|---|---|
| `search-query-string` | Query string search | `json` | `searchHits` |
| `search-facets` | Terms facet | `json` | `facets` |
| `search-hybrid` | Hybrid text and vector search | `json` | `searchHits` |
| `vector-similarity` | kNN vector search | `json` | `vectorNeighbors` |
| `vector-filtered` | Filtered kNN search | `json` | `vectorNeighbors` |
| `vector-health` | Index mapping | `text` | `json` |

## ABI Calls

The driver handles these JSON requests today:

| Method | Response |
|---|---|
| `health` / `ping` | Connector health, engine id, ABI version, and driver link status. |
| `describe` / `capabilities` | Embedded manifest and connector config. |
| `manifest` | Raw `irodori.extension.json`. |
| `config` | Raw `connector.config.json`. |
| `connect` | Opens an HTTP client and validates the cluster root endpoint. |
| `query` | Runs SQL through the OpenSearch SQL plugin endpoint. |
| `metadata` | Loads index metadata from `_cat/indices` and `_mapping`. |
| `close` | Removes the cached native connection. |

## Development


Generated extension repositories share `../target` across sibling repositories so Rust dependencies are compiled once per checkout. DuckDB and MotherDuck are driver-linked by default; set `IRODORI_CONNECTOR_LINK_DUCKDB=0` only when you need metadata-only DuckDB-compatible scaffolds.


```sh
make check
make build
```

Release packages place platform-specific native artifacts under `dist/native`.
