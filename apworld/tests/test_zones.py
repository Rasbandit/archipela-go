from dataclasses import fields

from worlds.ap_go2.zones import Zone, build_zones


def test_zone_one_is_free_and_later_zones_need_keys() -> None:
    z = build_zones(3)
    assert [(x.id, x.keys_needed) for x in z] == [(1, 0), (2, 1), (3, 2)]


def test_zones_have_no_mode_or_tool() -> None:
    assert {f.name for f in fields(Zone)} == {"id", "keys_needed"}
