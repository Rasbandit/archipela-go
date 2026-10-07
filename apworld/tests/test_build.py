import json
import subprocess
import zipfile
from pathlib import Path

from worlds.ap_go2.constants import GAME_NAME

ROOT = Path(__file__).parents[2]


def test_build_produces_loadable_apworld() -> None:
    subprocess.run(["bash", str(ROOT / "scripts" / "build_apworld.sh")], check=True)  # noqa: S603, S607
    with zipfile.ZipFile(ROOT / "dist" / "ap_go2.apworld") as z:
        entries = set(z.namelist())
        manifest = json.loads(z.read("ap_go2/archipelago.json"))
    assert "ap_go2/__init__.py" in entries
    assert not any("__pycache__" in e or e.startswith("ap_go2/tests") for e in entries)
    assert manifest["game"] == GAME_NAME
    # Added by Archipelago's own builder; 0.7.0 refuses apworlds without them.
    assert manifest["compatible_version"] == 7
    assert "version" in manifest
    assert manifest["world_version"] == "0.2.0"
