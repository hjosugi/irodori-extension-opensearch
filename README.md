<!-- i18n: language-switcher -->
[English](README.md) | [日本語](README.ja.md)

# OpenSearch Connector

Native Irodori Table connector extension for OpenSearch.

This crate packages the connector metadata, native ABI exports, and driver implementation used by the Irodori extension marketplace.

## Connector

- Extension ID: `irodori.opensearch`
- Engine ID: `openSearch`
- Wire protocol: `search`
- Default port: `9200`
- Native ABI: `irodori.connector.native.v1`
- Driver linked: `yes`
- Marketplace visibility: `public`
- Package version: `0.1.4`

The package uses the connector metadata and native driver directly; no desktop adapter source snapshot is required.

Connector metadata lives in `connector.config.json` and `irodori.extension.json`.
The Rust crate exports the native ABI from `src/lib.rs`, uses `irodori-connector-abi` for shared JSON/buffer helpers, and keeps connector behavior in `src/driver.rs`.

## Connection Metadata

- Endpoint modes: `hostPort`, `connectionString`
- Transport modes: `direct`, `sshTunnel`, `socks5Proxy`, `httpConnectProxy`, `proxyChain`
- TLS supported: `yes`
- TLS required by default: `no`
- Custom driver options: `yes`

### Endpoint Fields

| Field | Label | Type | Required |
| --- | --- | --- | --- |
| `host` | Host | `string` | yes |
| `port` | Port | `number` | no |
| `database` | Database | `string` | no |

## Authentication

The connector advertises these authentication modes so clients can render the right credential fields. Driver-specific or provider-specific values can still be passed through `options` when needed.

| Auth method | Label | Kind | Secret purposes |
| --- | --- | --- | --- |
| `none` | No authentication | `none` | none |
| `connectionString` | Connection string / DSN | `connectionString` | none |
| `basic` | Basic authentication | `userPassword` | `password` |
| `apiKey` | API key | `apiKey` | `token` |
| `bearerToken` | Bearer token | `token` | `token` |
| `jwt` | JWT bearer token | `token` | `token` |
| `accessToken` | Access token | `token` | `token` |
| `oauth2` | OAuth 2.0 | `oauth2` | `token` |
| `clientCertificate` | Client certificate / mTLS | `certificate` | `privateKey`, `privateKeyPassphrase` |
| `awsDefaultCredentialsChain` | AWS default credential chain | `iam` | none |
| `awsSigV4` | AWS SigV4 | `iam` | `token` |
| `awsProfile` | AWS shared config profile | `iam` | none |
| `awsSso` | AWS IAM Identity Center / SSO | `iam` | `token` |
| `webIdentity` | AWS web identity | `iam` | `token` |
| `awsAssumeRole` | AWS STS assume role | `iam` | `token` |
| `customDriverOptions` | Custom driver options | `custom` | `password`, `token`, `privateKey`, `privateKeyPassphrase` |

## Experience Metadata

- Domains: `search`, `vector`
- Result views: `searchHits`, `facets`, `json`, `table`, `vectorNeighbors`
- Object types: `indexes`, `mappings`, `aliases`, `templates`, `analyzers`, `collections`, `vectors`, `payloadFields`, `partitions`, `namespaces`
- Inspired by: OpenSearch Dashboards Discover, OpenSearch aggregations, OpenSearch hybrid search, OpenSearch k-NN search

| Workflow | Result view | Templates |
| --- | --- | --- |
| Discover documents | `searchHits` | `search-query-string` |
| Facet breakdown | `facets` | `search-facets` |
| Hybrid search | `searchHits` | `search-hybrid` |
| Similarity search | `vectorNeighbors` | `vector-similarity` |
| Filtered ANN search | `vectorNeighbors` | `vector-filtered` |
| Collection or index health | `table` | `vector-health` |

| Template | Label | Language | Result view |
| --- | --- | --- | --- |
| `search-query-string` | Query string search | `json` | `searchHits` |
| `search-facets` | Terms facet | `json` | `facets` |
| `search-hybrid` | Hybrid text and vector search | `json` | `searchHits` |
| `vector-similarity` | kNN vector search | `json` | `vectorNeighbors` |
| `vector-filtered` | Filtered kNN search | `json` | `vectorNeighbors` |
| `vector-health` | Index mapping | `text` | `json` |

## Native ABI Calls

| Method | Response |
| --- | --- |
| `health` | Returns connector health, engine id, ABI version, and driver status. |
| `describe` | Returns the embedded manifest and connector config. |
| `manifest` | Returns raw `irodori.extension.json`. |
| `config` | Returns raw `connector.config.json`. |
| `connect` | Opens and validates a native connector connection. |
| `query` | Runs a connector query and returns structured rows or JSON results. |
| `metadata` | Reads schemas, tables, columns, indexes, collections, or equivalent metadata. |
| `close` | Closes and removes a cached native connection. |

## Development

All extension crates in this checkout share `../target` so dependencies compile once across sibling repositories.

```sh
make check
make build
```

Release packages place platform-specific native artifacts under `dist/native`.

## License

0BSD. You can use, copy, modify, and distribute this project for almost any purpose.
