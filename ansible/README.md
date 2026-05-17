# Vey Ansible

This directory contains Ansible playbooks and roles for deploying, managing, and configuring the `vey-proxy` service and its associated configurations across different environments.

## Directory Structure

*   **`*.yml`**: Top-level playbooks corresponding to different deployment scenarios and components.
*   **`roles/`**: Contains all Ansible roles used by the playbooks.

## Usage

To use these playbooks, you typically run `ansible-playbook` specifying the target playbook, inventory, and specific tags for the tasks you want to execute.

### Common Tags

The playbooks and roles use tags to control which tasks are executed. By default, most tasks are skipped unless their specific tag is provided.

Some common tags include:

*   `deploy`: Installs or updates the `vey-proxy` binaries and base configurations.
*   `upgrade`: Upgrades the `vey-proxy` binary to a new version.
*   `config-all`: Generates and pushes all configuration files for the specific role.
*   `restart`: Restarts the `vey-proxy` service.
*   `stop`: Stops the `vey-proxy` service.
*   `query-version`: Checks the currently installed or running version of `vey-proxy`.
*   `clean-config`: Removes old or unused configuration files.

### Finding Action Tags

Because these playbooks use tags extensively to control which tasks execute, you may want to discover all available tags for a specific playbook. You can do this by running Ansible with the `--list-tags` flag:

```bash
ansible-playbook -i your_inventory playbook_name.yml --list-tags
```

Alternatively, you can manually inspect a role's `tasks/main.yml` file (e.g., `roles/vey-proxy/tasks/main.yml`), which explicitly defines the available tags. Notice that most tasks are marked with `never` by default, meaning their specific action tag must be provided via the `-t` argument in order to execute them.

### Examples

**1. Deploy `border-classic` role to all hosts:**
```bash
ansible-playbook -i your_inventory border-classic.yml -t deploy
```

**2. Update configurations for `border-transit` and restart:**
```bash
ansible-playbook -i your_inventory border-transit.yml -t config-all,restart
```

**3. Check running version of `vey-proxy` on benchmark hosts:**
```bash
ansible-playbook -i your_inventory benchmark.yml -t query-version
```

## Available Roles

Each role handles a specific part of the setup or a specific proxy configuration profile:

*   **[`vey-proxy`](roles/vey-proxy/)**: The base role for managing the `vey-proxy` binary, daemon, and basic operations.
*   **[`benchmark`](roles/benchmark/)**: Configuration profile for benchmark environments.
*   **[`border-classic`](roles/border-classic/)**: Configuration profile for classic border proxy deployments.
*   **[`border-concise`](roles/border-concise/)**: Configuration profile for concise border proxy deployments.
*   **[`border-stream`](roles/border-stream/)**: Configuration profile for stream proxy deployments.
*   **[`border-transit`](roles/border-transit/)**: Configuration profile for transit proxy deployments.
*   **[`common`](roles/common/)**: Common tasks shared across different proxy configuration roles.
*   **[`rsyslog`](roles/rsyslog/)**: Configures system logging (rsyslog) for the proxy logs.
*   **[`sysconf-idc`](roles/sysconf-idc/)**: Configures IDC specific system settings.

For more details on a specific role, please check the `README.md` in its respective directory.
