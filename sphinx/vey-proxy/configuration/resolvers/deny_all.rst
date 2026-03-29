.. _configuration_resolver_deny_all:

deny_all
========

Resolver that always fails.

Use it when you want resolution to be disabled explicitly, or as a safe default
for escapers that should never perform DNS lookups.

Only the common ``name`` and ``type`` fields are accepted by the loader.
