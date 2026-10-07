import subprocess
import zipfile
from pathlib import Path

ROOT = Path(__file__).parents[2]


def test_build_produces_valid_apworld() -> None:
    subprocess.run(["bash", str(ROOT / "scripts" / "build_apworld.sh")], check=True)  # noqa: S603, S607
    with zipfile.ZipFile(ROOT / "dist" / "ap_go2.apworld") as z:
        entries = set(z.namelist())
    assert "ap_go2/archipelago.json" in entries
    assert "ap_go2/__init__.py" in entries
    assert not any("__pycache__" in e or e.startswith("ap_go2/tests") for e in entries)
