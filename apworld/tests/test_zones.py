from worlds.ap_go2.zones import build_zones, items_to_enter, required_tools


def test_zone_one_is_free_and_toolless() -> None:
    z = build_zones(["bike", "bike", "drive"])
    assert (z[0].keys_needed, z[0].tool) == (0, None)
    assert (z[1].keys_needed, z[1].tool) == (1, None)  # same mode as zone 1
    assert (z[2].keys_needed, z[2].tool) == (2, "Car")


def test_tools_only_for_modes_other_than_zone_one() -> None:
    assert required_tools(["walk"]) == []
    assert required_tools(["walk", "run", "bike", "drive"]) == ["Running Shoes", "Bike", "Car"]
    assert required_tools(["run", "run", "bike"]) == ["Bike"]
    assert required_tools(["drive", "walk", "walk"]) == []  # walking needs no tool
    assert required_tools(["walk", "bike", "bike"]) == ["Bike"]  # distinct


def test_items_to_enter_accumulates() -> None:
    zones = build_zones(["walk", "bike", "drive"])
    assert items_to_enter(zones, 1) == (0, set())
    assert items_to_enter(zones, 2) == (1, {"Bike"})
    assert items_to_enter(zones, 3) == (2, {"Bike", "Car"})
