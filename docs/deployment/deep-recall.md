# Deep recall deployment

Deep is an explicit, opt-in recall mode for difficult queries. Fast remains the default, and MemPhant never auto-escalates a request into Deep.

## Data egress and privacy

Setting `MEMPHANT_DEEP=on` authorizes the serving process to send the query and the complete bodies of policy-authorized episode/resource sources to OpenRouter for processing by Azure. The workspace exists only in memory; the agent has no shell, web, arbitrary filesystem, write, or memory-mutation tool.

Every model request requires Azure routing, Zero Data Retention, denied provider data collection, and support for every requested parameter. ZDR limits retention; it does **not** guarantee geographic residency. Workloads with residency requirements need a separately verified regional route and must leave Deep off until that route is approved.

Cancellation drops the streaming HTTP request so Azure can stop processing and billing. A cancelled generation whose final provider usage cannot be reconciled is reported with an explicit unsettled token/spend upper bound; MemPhant never reports that possible charge as zero.

## Configuration

Deep is off when `MEMPHANT_DEEP` is unset or exactly `off`. The only enabled value is exact `on`; other values fail startup. When enabled, set:

- `OPENROUTER_API_KEY`
- `MEMPHANT_DEEP_MODEL` to one exact model ID (floating aliases such as `latest` are rejected)
- `MEMPHANT_DEEP_PROMPT_PATH` to an immutable prompt file
- `MEMPHANT_DEEP_PROVIDERS=azure`
- `MEMPHANT_DEEP_INPUT_PRICE_MICROS_PER_MILLION`
- `MEMPHANT_DEEP_OUTPUT_PRICE_MICROS_PER_MILLION`

`MEMPHANT_DEEP_OPENROUTER_BASE_URL` defaults to `https://openrouter.ai/api/v1`. An override is a durable private/regional gateway contract: MemPhant sends that gateway the API key, query, and authorized source bodies, so the operator must verify that it preserves OpenRouter's streaming, generation-metadata, ZDR, data-deny, and Azure-routing semantics. Overrides must use HTTPS; HTTP is accepted only for a loopback test gateway. Credentials, query strings, and fragments in the URL are rejected. The endpoint is covered by the config hash.

The shipped operating point is one 120-second wall deadline, at most 24 completed model responses, 96,000 cumulative provider input tokens, USD 0.30 maximum spend, and 4,096 maximum completion tokens. Retries, tool turns, and final usage reconciliation share those ceilings. Model, provider, prompt, prices, limits, transport endpoints, retry policy, and tool-output bounds are construction-time facts covered by the prompt/config hashes.

Only server and MCP recall services install the provider through `build_service`. The worker never reads this configuration or sends source data to a model.
