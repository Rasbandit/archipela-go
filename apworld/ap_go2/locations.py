from . import names
from .constants import ID_OFFSET, MAX_TRIPS

LOCATION_NAME_TO_ID: dict[str, int] = {
    names.trip_name(n): ID_OFFSET + n for n in range(1, MAX_TRIPS + 1)
}
