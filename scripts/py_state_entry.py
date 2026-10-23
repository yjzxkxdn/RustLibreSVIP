"""Run `libresvip.cli` with `platformdirs` redirected to a temporary state dir.

`platformdirs` on Windows resolves known folders through the shell API and
ignores `LOCALAPPDATA`, so the parity harness cannot isolate state through the
environment alone. This wrapper is executed as a module by the harness:

    LIBRESVIP_TEST_STATE=<dir> python -m py_state_entry <cli args...>

It must be imported before `libresvip` so `libresvip.core.constants.app_dir`
picks up the redirected paths.
"""

from __future__ import annotations

import os
import pathlib
import sys

import platformdirs


class _StateDirs:
    def __init__(self, *args: object, **kwargs: object) -> None:
        root = pathlib.Path(os.environ["LIBRESVIP_TEST_STATE"])
        self._root = root

    @property
    def user_config_path(self) -> pathlib.Path:
        return self._root / "config"

    @property
    def user_data_dir(self) -> pathlib.Path:
        return self._root / "data"

    @property
    def user_cache_dir(self) -> pathlib.Path:
        return self._root / "cache"

    @property
    def user_log_dir(self) -> pathlib.Path:
        return self._root / "logs"


platformdirs.AppDirs = _StateDirs  # type: ignore[assignment,misc]

from libresvip.cli.app import app  # noqa: E402

if __name__ == "__main__":
    app()
