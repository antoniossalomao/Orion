import io
import stat
import zipfile

import pytest

from orion.extensions.archives import archive_snapshot, install_archive
from orion.extensions.installer import Installer, folder_snapshot
from orion.extensions.plugins import PluginError, PluginStore


def zip_bytes(files, *, compress=zipfile.ZIP_STORED):
    stream = io.BytesIO()
    with zipfile.ZipFile(stream, "w", compression=compress) as archive:
        for name, content in files:
            archive.writestr(name, content)
    return stream.getvalue()


def test_zip_and_folder_same_hash_validator_no_execution(bundle, tmp_path):
    package = folder_snapshot(bundle)
    data = zip_bytes(package.files.items())
    assert archive_snapshot(data, "research.orion-plugin.zip").digest == package.digest
    store = PluginStore(tmp_path / "registry.db")
    try:
        installer = Installer(tmp_path / "extensions", store)
        result = install_archive(installer, data, "research.zip")
        assert result["state"] == "disabled"
        assert not list(installer.root.rglob("*.ran"))
    finally:
        store.close()


@pytest.mark.parametrize(
    "name",
    ["../../escaped", "/tmp/escaped", "C:/escaped", "a\\b", "a:ads", "NUL.txt", "a/../escape"],
)
def test_zip_slip_and_windows_paths_never_write_outside(tmp_path, name):
    with pytest.raises(PluginError):
        archive_snapshot(zip_bytes([(name, b"x")]), "archive.zip")
    assert list(tmp_path.iterdir()) == []


def test_symlink_collision_bomb_and_corrupt():
    info = zipfile.ZipInfo("link")
    info.create_system = 3
    info.external_attr = (stat.S_IFLNK | 0o777) << 16
    cases = [
        zip_bytes([(info, b"/etc/passwd")]),
        zip_bytes([("a", b"x"), ("A", b"y")]),
        zip_bytes([("a", b"x"), ("a/b", b"y")]),
        zip_bytes([("large", b"x" * 100000)], compress=zipfile.ZIP_DEFLATED),
        zip_bytes([(f"file{i}", b"x") for i in range(1025)]),
        b"not zip",
    ]
    for data in cases:
        with pytest.raises(PluginError):
            archive_snapshot(data, "arquivo.zip")
    with pytest.raises(PluginError, match="archive_extension_unsupported"):
        archive_snapshot(zip_bytes([]), "arquivo.tar")


def test_expanded_size_and_bad_crc_rejected(bundle):
    package = folder_snapshot(bundle)
    with pytest.raises(PluginError):
        archive_snapshot(
            zip_bytes([*package.files.items(), ("big", b"x" * 2000001)]), "arquivo.zip"
        )
    data = bytearray(zip_bytes(package.files.items()))
    # Alterar o primeiro payload preserva directory mas rompe CRC.
    data[30 + len("manifest.json")] ^= 1
    with pytest.raises(PluginError, match="archive_corrupt"):
        archive_snapshot(bytes(data), "arquivo.zip")
