# Benchmark Ansible Role

The `benchmark` role is used to configure and manage `vey-proxy` for benchmarking purposes.

It builds upon the base `vey-proxy` role and adds its own specific configuration templates and tasks.

## Tasks & Tags

*   `deploy`: Deploys the benchmark configuration and the proxy binary.
*   `query-version`: Checks the currently running version of the proxy.
*   `upgrade`: Upgrades the proxy binary.
*   `restart`: Restarts the proxy service.
*   `config-all`: Generates and applies all benchmark-specific configuration files and reloads the service.

## Variables

The following variables can be configured (with their defaults defined in `defaults/main.yml`):

*   `benchmark_tls_name`: TLS server name for the benchmark proxy (default: `bench.example.net`).
*   `benchmark_http_port`: Port for HTTP proxy benchmarking (default: `8080`).
*   `benchmark_https_port`: Port for HTTPS proxy benchmarking (default: `9080`).
*   `benchmark_tcp_port`: Port for TCP proxy benchmarking (default: `8090`).
*   `benchmark_tls_port`: Port for TLS proxy benchmarking (default: `9090`).
*   `daemon_group`: Set to `benchmark` to define the instance name.

## Usage Example

You can apply this role using the `benchmark.yml` playbook:

```bash
# To generate and apply configurations
ansible-playbook -i inventory benchmark.yml -t config-all

# To upgrade the proxy
ansible-playbook -i inventory benchmark.yml -t upgrade
```
