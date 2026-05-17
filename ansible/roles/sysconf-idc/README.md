# Sysconf IDC Ansible Role

The `sysconf-idc` role configures system-level settings and configurations specific to the IDC (Internet Data Center) environment where the `vey-proxy` instances are deployed.

This may include network tuning, system limits, and other OS-level configurations needed for optimal proxy performance in a specific data center context.

## Tasks & Tags

The following tags are available to control which configuration tasks are run:

*   `config-sysctl`: Applies kernel `sysctl` configurations optimized for proxying.
*   `config-repo`: Configures the system package manager (e.g. `yum`, `apt`) repositories and proxies.
*   `config-all`: Runs both `config-sysctl` and `config-repo` tasks.
*   `config`: An alias that runs the `config-sysctl` tasks.

Note that these configuration tasks are typically tagged with `never` by default and must be explicitly specified (e.g. using `-t config-all`).

## Variables

The following variables can be configured to control the environment and system configurations:

*   `ansible_run_env`: A dictionary for setting environment variables during the playbook execution (default: `{}`).
*   `vey_repo_proxy`: Proxy URL used for package managers (like `yum`) when deploying or configuring system dependencies (default: `_none_`).

## Usage Example

Because tasks are tagged with `never` by default, you must specify the desired tag (e.g., `config-all`) to apply the configurations:

```bash
# Apply both sysctl and repo configurations
ansible-playbook -i inventory sysconf-idc.yml -t config-all

# Apply only sysctl configurations
ansible-playbook -i inventory sysconf-idc.yml -t config-sysctl
```
