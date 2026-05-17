# Vey-Proxy Ansible Role

The `vey-proxy` role is the foundational role responsible for installing, managing, and performing basic operations on the `vey-proxy` binary and its systemd service.

This role must be included before any configuration-specific roles (such as `border-classic`, `benchmark`, etc.) in your playbook to properly install the binary and base daemon configuration. While other configuration roles *do* import this role, they only do so for specific life-cycle actions (like starting or restarting the service), not for the initial deployment.

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

### 1. Primary Deployment

When deploying a new proxy instance, you must run the `vey-proxy` role before your specific profile role. You can do this by creating a playbook that lists them in order:

```yaml
---
- hosts: my_proxies
  roles:
    - role: vey-proxy
    - role: border-classic
```

Then deploy using:
```bash
ansible-playbook -i inventory my_playbook.yml -t deploy
```

### 2. Imported Actions

Other configuration roles will import `vey-proxy` behind the scenes for specific actions (such as restarting or querying versions) like this:

```yaml
- ansible.builtin.import_role:
    name: vey-proxy
    tasks_from: start-after-deploy
```
