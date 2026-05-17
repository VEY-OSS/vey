# Vey-Proxy Ansible Role

The `vey-proxy` role is the foundational role responsible for installing, managing, and performing basic operations on the `vey-proxy` binary and its systemd service.

This role is typically imported and used by other configuration-specific roles (like `border-classic`, `benchmark`, etc.) rather than being run directly for configurations.

## Tasks & Tags

This role defines several tasks that can be executed using Ansible tags. Note that many tasks are tagged with `never` by default, meaning they must be explicitly requested.

*   `query-version`: Queries the installed version of `vey-proxy`.
*   `query-running-version`: Queries the currently running version of `vey-proxy`.
*   `deploy`: Deploys the `vey-proxy` binary and base service configuration.
*   `config-log`: Configures logging directories and settings.
*   `uninstall-daemon`: Stops and uninstalls the systemd daemon.
*   `upgrade`: Upgrades the `vey-proxy` binary.
*   `restart`: Restarts the `vey-proxy` systemd service.
*   `stop`: Stops the `vey-proxy` systemd service.
*   `clean-config`: Removes old configuration files from the configuration directory.

## Variables

The following variables can be configured to control the role's behavior (often defined in `defaults/main.yml` or passed via inventory):

*   `daemon_group`: The name of the proxy instance, which dictates the systemd service name and configuration directory (e.g., `/etc/vey-proxy/{{ daemon_group }}/`). Required by playbooks.
*   `allowed_roles`: A list of role names allowed to be executed on the target host. Used for safety checks.
*   `enterprise_id`: Enterprise identifier (default: `32473`).
*   `proxy_log_dir`: The directory for proxy logs (default: `/var/log/vey-proxy`).
*   `proxy_log_udp_port`: UDP port for receiving logs (default: `1514`).
*   `proxy_log_rotate_count`: Number of rotated logs to keep (default: `7`).
*   `proxy_log_rotate_minsize`: Minimum size before rotating a log (default: `1G`).
*   `proxy_log_delaycompress`: Delay compression of rotated logs (default: `true`).

## Usage Example

Typically, this role is invoked via an `import_role` in other roles:

```yaml
- ansible.builtin.import_role:
    name: vey-proxy
    tasks_from: restart
  tags:
    - restart
```
