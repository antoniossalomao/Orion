"""ZIP de distribuição limitado; sem extractall, links, ZIP64 ou execução."""

from __future__ import annotations

import io
import stat
import struct
import unicodedata
import zipfile
import zlib
from pathlib import Path

from .installer import (
    MAX_EXPANDED,
    MAX_FILE,
    MAX_FILES,
    Installer,
    Package,
    fingerprint,
    package_path,
)
from .plugins import PluginError, parse_manifest

MAX_ARCHIVE = 16_000_000


def _check_directory(data: bytes) -> None:
    # Limitar central directory antes de ZipFile criar milhares de objetos ZipInfo.
    offset = data.rfind(b"PK\x05\x06", max(0, len(data) - 65557))
    if offset < 0 or offset + 22 > len(data):
        raise PluginError("archive_corrupt")
    _signature, disk, directory_disk, on_disk, count, size, start, comment = struct.unpack_from(
        "<4s4H2LH", data, offset
    )
    if (
        disk
        or directory_disk
        or on_disk != count
        or count > MAX_FILES * 2
        or start + size != offset
        or offset + 22 + comment != len(data)
    ):
        raise PluginError("archive_format_or_size_unsupported")
    cursor, entries = start, 0
    while cursor < offset:
        if data[cursor : cursor + 4] != b"PK\x01\x02" or cursor + 46 > offset:
            raise PluginError("archive_corrupt")
        name, extra, note = struct.unpack_from("<3H", data, cursor + 28)
        if name > 1024 or extra > 4096 or note > 1024:
            raise PluginError("archive_metadata_limit")
        cursor += 46 + name + extra + note
        entries += 1
        if entries > MAX_FILES * 2:
            raise PluginError("archive_entry_limit")
    if cursor != offset or entries != count:
        raise PluginError("archive_corrupt")


def archive_snapshot(data: bytes, filename: str) -> Package:
    if not filename.lower().endswith(".zip"):
        raise PluginError("archive_extension_unsupported")
    if len(data) > MAX_ARCHIVE:
        raise PluginError("archive_size_limit")
    _check_directory(data)
    files, canonical_files, names = {}, set(), set()
    total = 0
    try:
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            for info in archive.infolist():
                if "\0" in info.orig_filename or info.flag_bits & 1:
                    raise PluginError("archive_encrypted_or_invalid")
                name = package_path(info.filename.rstrip("/") if info.is_dir() else info.filename)
                canonical = unicodedata.normalize("NFC", name).casefold()
                if canonical in names:
                    raise PluginError("archive_name_collision")
                names.add(canonical)
                mode = stat.S_IFMT(info.external_attr >> 16)
                if mode not in (0, stat.S_IFREG, stat.S_IFDIR):
                    raise PluginError("archive_special_file_forbidden")
                if info.compress_type not in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED):
                    raise PluginError("archive_compression_unsupported")
                if info.is_dir():
                    if info.file_size:
                        raise PluginError("archive_directory_data")
                    continue
                if mode == stat.S_IFDIR:
                    raise PluginError("archive_entry_invalid")
                canonical_files.add(canonical)
                if (
                    info.file_size > MAX_FILE
                    or info.file_size > 200 * max(1, info.compress_size)
                    or len(files) >= MAX_FILES
                ):
                    raise PluginError("archive_expansion_limit")
                total += info.file_size
                if total > MAX_EXPANDED:
                    raise PluginError("archive_expansion_limit")
                with archive.open(info) as stream:
                    content = stream.read(MAX_FILE + 1)
                if len(content) != info.file_size:
                    raise PluginError("archive_size_mismatch")
                files[name] = content
    except (zipfile.BadZipFile, OSError, RuntimeError, EOFError, UnicodeError, zlib.error) as error:
        raise PluginError("archive_corrupt") from error
    for name in names:
        parts = name.split("/")
        if any("/".join(parts[:i]) in canonical_files for i in range(1, len(parts))):
            raise PluginError("archive_file_directory_collision")
    if "manifest.json" not in files:
        raise PluginError("manifest_missing")
    return Package(parse_manifest(files["manifest.json"]), fingerprint(files), files)


def install_archive(
    installer: Installer, data: bytes, filename: str, *, update: bool = False
) -> dict:
    # Exatamente o mesmo validador/staging e publicação da pasta local.
    return installer.install_snapshot(
        archive_snapshot(data, filename), origin="archive", update=update
    )


def archive_file(installer: Installer, path: Path, *, update: bool = False) -> dict:
    if path.is_symlink() or not path.is_file() or path.stat().st_size > MAX_ARCHIVE:
        raise PluginError("archive_source_invalid")
    with path.open("rb") as stream:
        data = stream.read(MAX_ARCHIVE + 1)
    return install_archive(installer, data, path.name, update=update)
