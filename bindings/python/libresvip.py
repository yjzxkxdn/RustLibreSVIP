"""Python binding for the LibreSVIP conversion engine (via the C ABI).

    pip install ctypes          # already in the standard library
    # build the shared library once:
    #   cargo build --release -p libresvip-ffi
    # then point LIBRESVIP_LIB at it, or copy it next to this file.

    >>> from libresvip import LibreSVIP
    >>> engine = LibreSVIP()
    >>> engine.formats()[:2]                      # doctest: +SKIP
    [{'id': 'acep', ...}, ...]
    >>> data = engine.convert(open("song.ustx", "rb").read(), "ustx", "svp")
    >>> open("song.svp", "wb").write(data)        # doctest: +SKIP

The engine is thread-safe: each call converts independently and the warnings
returned by :meth:`convert` belong to the calling thread.
"""

from __future__ import annotations

import ctypes
import json
import os
import sys
from typing import Any

__all__ = ["LibreSVIP", "LibreSVIPError", "load"]

_LIBRARY_NAMES = {
    "win32": ("libresvip.dll", "libresvip-ffi.dll"),
    "darwin": ("liblibresvip.dylib", "libresvip.dylib"),
}
_DEFAULT_NAMES = ("liblibresvip.so", "libresvip.so")


class LibreSVIPError(RuntimeError):
    """Raised when the engine rejects a conversion."""


def _library_names() -> tuple[str, ...]:
    platform = "win32" if sys.platform == "win32" else "darwin" if sys.platform == "darwin" else None
    return _LIBRARY_NAMES.get(platform, _DEFAULT_NAMES)


def _library_candidates(explicit: str | None) -> list[str]:
    if explicit:
        return [explicit]
    candidates: list[str] = []
    from_env = os.environ.get("LIBRESVIP_LIB")
    if from_env:
        candidates.append(from_env)
    here = os.path.dirname(os.path.abspath(__file__))
    roots = [
        here,
        os.path.join(here, "..", "..", "target", "release"),
        os.path.join(here, "..", "..", "target", "debug"),
        os.path.join(here, "..", "..", "target", "fast"),
    ]
    for root in roots:
        for name in _library_names():
            candidates.append(os.path.normpath(os.path.join(root, name)))
    return candidates


def load(library: str | None = None) -> "LibreSVIP":
    """Loads the engine from an explicit path, ``LIBRESVIP_LIB`` or the usual
    build directories."""
    return LibreSVIP(library)


def _open_library(path: str | None) -> ctypes.CDLL:
    tried: list[str] = []
    for candidate in _library_candidates(path):
        tried.append(candidate)
        if os.path.isfile(candidate):
            return ctypes.CDLL(candidate)
    raise LibreSVIPError(
        "libresvip shared library not found; build it with "
        "`cargo build --release -p libresvip-ffi` or set LIBRESVIP_LIB.\n"
        "tried:\n  " + "\n  ".join(tried)
    )


class LibreSVIP:
    """A loaded engine instance."""

    def __init__(self, library: str | None = None):
        self._lib = _open_library(library)
        self._bind()

    # -- plumbing ---------------------------------------------------------
    def _bind(self) -> None:
        lib = self._lib
        pointer = ctypes.POINTER(ctypes.c_ubyte)
        lib.libresvip_version.restype = ctypes.c_char_p
        lib.libresvip_version.argtypes = []
        lib.libresvip_free.restype = None
        lib.libresvip_free.argtypes = [pointer, ctypes.c_size_t]
        lib.libresvip_formats_json.restype = ctypes.c_int32
        lib.libresvip_formats_json.argtypes = [
            ctypes.POINTER(pointer),
            ctypes.POINTER(ctypes.c_size_t),
        ]
        lib.libresvip_options_json.restype = ctypes.c_int32
        lib.libresvip_options_json.argtypes = [
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.POINTER(pointer),
            ctypes.POINTER(ctypes.c_size_t),
        ]
        lib.libresvip_convert.restype = ctypes.c_int32
        lib.libresvip_convert.argtypes = [
            pointer,
            ctypes.c_size_t,
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.POINTER(pointer),
            ctypes.POINTER(ctypes.c_size_t),
        ]
        lib.libresvip_warnings_json.restype = ctypes.c_int32
        lib.libresvip_warnings_json.argtypes = [
            ctypes.POINTER(pointer),
            ctypes.POINTER(ctypes.c_size_t),
        ]

    def _take(self, pointer, length: int) -> bytes:
        try:
            return ctypes.string_at(pointer, length) if pointer and length else b""
        finally:
            if pointer:
                self._lib.libresvip_free(pointer, length)

    def _call_buffer(self, function, *args: Any) -> bytes:
        pointer = ctypes.POINTER(ctypes.c_ubyte)()
        length = ctypes.c_size_t(0)
        status = function(*args, ctypes.byref(pointer), ctypes.byref(length))
        payload = self._take(pointer, length.value)
        if status != 0:
            raise LibreSVIPError(self._decode(payload) or f"engine returned {status}")
        return payload

    @staticmethod
    def _decode(payload: bytes) -> str:
        return payload.decode("utf-8", "replace")

    # -- public API -------------------------------------------------------
    @property
    def version(self) -> str:
        """Engine version, e.g. ``"1.2.0"``."""
        return self._lib.libresvip_version().decode("ascii", "replace")

    def formats(self) -> list[dict[str, Any]]:
        """Every registered format with its id, name, suffixes and features."""
        return json.loads(self._call_buffer(self._lib.libresvip_formats_json))

    def options(self, input_format: str, output_format: str) -> dict[str, Any]:
        """Option schema (``{"input": [...], "output": [...]}``) for a pair."""
        raw = self._call_buffer(
            self._lib.libresvip_options_json,
            input_format.encode("utf-8"),
            output_format.encode("utf-8"),
        )
        return json.loads(raw)

    def convert(
        self,
        data: bytes,
        input_format: str,
        output_format: str,
        input_options: dict[str, Any] | None = None,
        output_options: dict[str, Any] | None = None,
    ) -> bytes:
        """Converts one project file and returns the output bytes.

        ``input_options`` / ``output_options`` are partial overrides; fields that
        are left out keep the plugin's default.
        """
        raw = bytes(data)
        buffer_type = ctypes.c_ubyte * (len(raw) or 1)
        input_buffer = buffer_type.from_buffer_copy(raw) if raw else buffer_type()
        in_json = json.dumps(input_options).encode("utf-8") if input_options else None
        out_json = json.dumps(output_options).encode("utf-8") if output_options else None
        return self._call_buffer(
            self._lib.libresvip_convert,
            ctypes.cast(input_buffer, ctypes.POINTER(ctypes.c_ubyte)),
            len(raw),
            input_format.encode("utf-8"),
            output_format.encode("utf-8"),
            in_json,
            out_json,
        )

    def warnings(self) -> list[dict[str, str]]:
        """Warnings raised by the last :meth:`convert` call on this thread."""
        return json.loads(self._call_buffer(self._lib.libresvip_warnings_json))

    def convert_file(
        self,
        source: str,
        destination: str,
        output_format: str,
        input_format: str | None = None,
    ) -> list[dict[str, str]]:
        """Converts ``source`` into ``destination`` (format guessed from the
        input suffix when omitted) and returns the warnings."""
        if input_format is None:
            suffix = os.path.splitext(source)[1].lstrip(".").lower()
            input_format = _format_for_suffix(self, suffix)
            if input_format is None:
                raise LibreSVIPError(f"no format registered for .{suffix}")
        with open(source, "rb") as handle:
            data = handle.read()
        output = self.convert(data, input_format, output_format)
        with open(destination, "wb") as handle:
            handle.write(output)
        return self.warnings()


def _format_for_suffix(engine: LibreSVIP, suffix: str) -> str | None:
    for entry in engine.formats():
        if suffix in entry["suffixes"]:
            return entry["id"]
    return None
