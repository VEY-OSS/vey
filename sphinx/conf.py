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
    "g3tiles": {
        "path": "g3tiles",
    },
    "statsd": {
        "path": "vey-statsd",
    },
    "keyless": {
        "path": "vey-keyless",
    },
}

# Common options
html_theme = 'sphinx_rtd_theme'
