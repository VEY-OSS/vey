# Configuration file for the Sphinx documentation builder.

from docutils import nodes
from docutils.parsers.rst import Directive


class AvailabilityDirective(Directive):
    has_content = True

    def run(self):
        container = nodes.admonition(classes=["admonition", "availability"])
        container += nodes.title(text="Availability")
        self.state.nested_parse(self.content, self.content_offset, container)
        return [container]


def setup(app):
    app.add_directive("availability", AvailabilityDirective)
    return {
        "parallel_read_safe": True,
        "parallel_write_safe": True,
    }


project = 'vey-values'
copyright = '2020 - %Y, Zhang Jingqiang'
author = 'Zhang Jingqiang'
release = '1.13.0'

extensions = []

templates_path = ['_templates']
exclude_patterns = ['_build', 'Thumbs.db', '.DS_Store']

html_theme = "sphinx_rtd_theme"
html_static_path = ['_static']

master_doc = 'index'
