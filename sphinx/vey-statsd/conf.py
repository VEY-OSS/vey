# Configuration file for the Sphinx documentation builder.
#
# For the full list of built-in configuration values, see the documentation:
# https://www.sphinx-doc.org/en/master/usage/configuration.html

from pathlib import Path
import os

project = 'vey-statsd'
copyright = '2025, Zhang Jingqiang'
author = 'Zhang Jingqiang'
release = '0.2.0'

# -- General configuration ---------------------------------------------------
# https://www.sphinx-doc.org/en/master/usage/configuration.html#general-configuration

extensions = [
    "sphinx.ext.intersphinx",
]

templates_path = ['_templates']
exclude_patterns = ['_build', 'Thumbs.db', '.DS_Store']

# -- Options for HTML output -------------------------------------------------
# https://www.sphinx-doc.org/en/master/usage/configuration.html#options-for-html-output

html_theme = 'sphinx_rtd_theme'
html_static_path = ['_static']

# -- Custom Options ----------------------------------------------------------

# Set the master document, which contains the root toctree directive.
# The default changed from 'contents' to 'index' from sphinx version 2.0,
# so we need to explicitly set it in order to be compatible with old versions.
master_doc = 'index'

_values_html_dir = (Path(__file__).resolve().parent.parent / "vey-values" / "_build" / "html").resolve()
_values_base_uri = os.environ.get("VEY_VALUES_DOC_BASE")
if not _values_base_uri:
    if os.environ.get("READTHEDOCS") == "True":
        _values_base_uri = "https://vey.readthedocs.io/projects/values/en/latest/"
    else:
        _values_base_uri = _values_html_dir.as_uri() + "/"

intersphinx_mapping = {
    "values": (_values_base_uri, str(_values_html_dir / "objects.inv")),
}
