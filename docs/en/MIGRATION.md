# Migrating from Apifox / Postman to RustFox

> 中文版见 [../MIGRATION.md](../MIGRATION.md).
> RustFox migrates through **open formats**: anything the source tool can export as
> OpenAPI / Postman Collection moves over in bulk; what can't (history, login
> sessions, runner configs) has a manual mapping below.

## 1. At a glance: what transfers automatically

| Apifox / Postman concept | RustFox equivalent | How |
| --- | --- | --- |
| Endpoints + groups | Endpoints + folders | OpenAPI / Postman Collection import builds the tree |
| Descriptions, params, bodies, response examples | Same content | Carried over by the import |
| Front URL / environment domain | Environment base_url (per-module base URLs for multi-service projects) | One line, manual |
| Environment / global variables | Environment / project / global variables (shared `{{name}}` syntax) | Copy over; Postman Environment files import directly |
| Bearer / Basic / API Key | Same auth types | Copy the secrets over (they never travel inside docs) |
| OAuth2 sessions | Four OAuth2 modes | Re-authorize to get a token |
| Mock data | Mock rules / response examples | Rebuild manually (rules match method + path + headers + body) |
| Post-processors / assertions | JSON test scripts (`pre_request` / `extract` / `assertions`) + test cases | Rewrite manually (§5 maps the concepts) |
| Request history, cookie sessions, runner configs | — | Not migrated; log in / configure again |

## 2. Export: get OpenAPI out of the source tool

**Apifox**: project settings → export OpenAPI (JSON or YAML; 3.0 / 3.1 both fine —
3.1 is normalized to a 3.0 subset on import, top-level `webhooks` are dropped).

**Postman**: Collection → Export v2.1 (JSON).

For huge projects, export/import per module in several passes (imports always append,
never overwrite existing data).

## 3. Import: three entries, same result

- Sidebar toolbar `+ New` dropdown → document import;
- Dashboard Dropzone / drag files onto project cards;
- Paste a stray cURL straight into the address bar (method / URL / headers / body /
  Basic Auth auto-detected into the current request).

Accepted formats: OpenAPI 3.0 / 3.1, Swagger 2.0, Postman Collection v2.1 (JSON or YAML).
After import, check two things: groups became folders, and (base_url + path) joins correctly.

## 4. Environments & variables: map them over

1. Create an environment, put the Apifox front URL into base_url (one base URL per
   module for multi-service projects);
2. Copy Apifox environment variables into RustFox variables verbatim — both sides use
   `{{name}}` (precedence: environment > project > global);
3. A Postman Environment file imports directly (bottom of the environment manager),
   name clashes get auto-suffixed;
4. Switch environments and send one legacy endpoint to confirm the domain and
   variables resolve.

## 5. Auth, mock, tests: rebuild with this mapping

- **Auth**: copy Bearer tokens / usernames-passwords / API Keys into the endpoint Auth;
  re-authorize OAuth2 in settings; cookie sessions don't transfer — log in once and
  replay takes over.
- **Mock**: Apifox mock expectations → RustFox mock rules (method + path + header +
  body matching, prioritized over response examples); plain fixed responses can be
  stored as "response examples" as fallback.
- **Assertions / post-processors**: Apifox "extract variable" maps to `extract`,
  "assertions" map to `assertions` (status / body contains / JSONPath / duration);
  store param combos as "test cases" and use "Run all" for collection regression.

## 6. Post-migration smoke test (5 minutes)

1. Switch through every environment, send one core endpoint each (200 means pass);
2. Run the assertion-bearing test-case collection once;
3. Export a smoke-test doc for the record ("Export smoke docs" in the test-case area);
4. Take a backup (Settings → backup JSON) — restores drill from it afterwards.

For import errors or lost fields, file an issue with a (redacted) source snippet:
<https://github.com/weihubeats/RustFox/issues>.
