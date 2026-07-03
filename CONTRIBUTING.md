# Contributing

MemPhant accepts contributions under Apache-2.0 with DCO sign-off. Add `Signed-off-by: Name <email>` to each commit you submit.

Keep contributions inside the public boundary:

- Do not add private Syndai imports, credentials, private fixtures, held-out benchmark data, production telemetry, hosted-control-plane logic, or customer data.
- Do not add MemPhant objects to `public`, `syndai`, or any host-app schema.
- Do not add a new public API verb, memory kind, DB provider mode, or SOTA claim without updating the owning spec first.
- Do not add code paths that make memory control flow. Memory is evidence.

Run the narrowest relevant checks before submitting, and include the exact commands and results in the build log when a STATUS item is updated.
