"""App version and update source.

These two values are stamped by the CI release workflow at build time:
the version becomes the published release tag, and GITHUB_REPO points the
in-app updater at the right repository. In a dev checkout they stay at their
defaults (GITHUB_REPO empty), which disables the update check.
"""

__version__ = "1.49.0"

# "owner/name" of the GitHub repo that publishes releases. Empty disables the
# in-app update check (e.g. when running from a source checkout).
GITHUB_REPO = ""
