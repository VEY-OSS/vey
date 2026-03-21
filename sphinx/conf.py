# File: docs/conf.py

extensions = [
    "multiproject",
]

# Define the projects that will share this configuration file.
multiproject_projects = {
    "project": {
        "path": "project",
    },
    "proxy": {
        "path": "vey-proxy",
    },
    "gateway": {
        "path": "vey-gateway",
    },
    "statsd": {
        "path": "vey-statsd",
    },
    "keyless": {
        "path": "vey-keyless",
    },
    "values": {
        "path": "vey-values",
    },
}

# Common options
html_theme = 'sphinx_rtd_theme'
