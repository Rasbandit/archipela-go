from BaseClasses import ItemClassification

from ap_go2 import names
from ap_go2.constants import ID_OFFSET, MAX_TRIPS
from ap_go2.items import ITEM_NAME_TO_ID, ITEM_TABLE
from ap_go2.locations import LOCATION_NAME_TO_ID
from ap_go2.options import ApGo2Options


def test_locations_cover_whole_pool() -> None:
    assert len(LOCATION_NAME_TO_ID) == MAX_TRIPS
    assert LOCATION_NAME_TO_ID[names.trip_name(1)] == ID_OFFSET + 1
    assert LOCATION_NAME_TO_ID[names.trip_name(MAX_TRIPS)] == ID_OFFSET + MAX_TRIPS


def test_item_ids_unique_and_complete() -> None:
    assert len(set(ITEM_NAME_TO_ID.values())) == len(ITEM_NAME_TO_ID)
    expected = {names.KEY, names.REDUCTION, names.SCOUTING, names.COLLECTION}
    expected |= set(names.ALL_TRAPS) | set(names.FILLERS) | set(names.LETTER_NAMES)
    assert set(ITEM_NAME_TO_ID) == expected


def test_defaults_make_a_world_that_feels_substantial() -> None:
    fields = {f.name: f.type for f in ApGo2Options.__dataclass_fields__.values()}
    assert fields["number_of_trips"].default == 100  # type: ignore[union-attr]
    assert fields["number_of_locks"].default == 3  # type: ignore[union-attr]


def test_classifications() -> None:
    assert ITEM_TABLE[names.KEY][1] == ItemClassification.progression
    assert ITEM_TABLE[names.REDUCTION][1] == ItemClassification.progression
    assert ITEM_TABLE[names.SCOUTING][1] == ItemClassification.useful
    assert ITEM_TABLE["Shuffle Trap"][1] == ItemClassification.trap
    assert ITEM_TABLE["Hydrate!"][1] == ItemClassification.filler
    assert ITEM_TABLE["Letter A"][1] == ItemClassification.progression
