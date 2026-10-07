"""Keep Archipelago's import-time dependency installer from running during tests."""

import ModuleUpdate  # type: ignore[import-not-found]

ModuleUpdate.update_ran = True
