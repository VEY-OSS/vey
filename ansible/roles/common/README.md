# Common Ansible Role

The `common` role contains tasks and configurations that are shared across various `vey-proxy` deployment profiles (like `benchmark`, `border-classic`, etc.).

Currently, it handles common networking and mapping configurations, such as merging IP map files.

## Tasks & Tags

This role is typically imported implicitly by other roles and usually runs with the `always` tag to ensure prerequisite configuration data is assembled.

*   `merge_ipmap`: Merges IP mapping definitions which might be required by proxy configurations.

## Variables

This role does not typically require any direct variables to be set by the user. It operates on existing network configuration variables (like the `ip_map` or network definitions) provided by the parent roles importing it.

## Usage Example

This role is not usually run standalone but is imported within other roles:

```yaml
- ansible.builtin.import_role:
    name: common
    tasks_from: merge_ipmap
  tags:
    - always
```
