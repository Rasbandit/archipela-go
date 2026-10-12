package dev.apgo2

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class BaseMapTest {
    @Test fun theBaseMapsOwnPlaceIconsAreHidden() {
        assertTrue("shops, parking, toilets, bus stops", BaseMap.hides("poi"))
        assertTrue("peaks would look like summit quests", BaseMap.hides("mountain_peak"))
        assertTrue(BaseMap.hides("aerodrome_label"))
    }

    @Test fun streetAndPlaceNamesAndAreasStay() {
        assertFalse(BaseMap.hides("transportation_name"))
        assertFalse(BaseMap.hides("place"))
        assertFalse(BaseMap.hides("park"))
        assertFalse(BaseMap.hides("water_name"))
        assertFalse("a layer with no source layer (the background) stays", BaseMap.hides(null))
    }

    @Test fun buildingsAreDrawnFlatNot3d() {
        assertTrue("3D extrusions cost GPU every frame at street zoom", BaseMap.hidesType("fill-extrusion"))
        assertFalse(BaseMap.hidesType("fill"))
        assertTrue("the flat footprint carries on where 3D took over", BaseMap.flatBuildings("fill", "building"))
        assertFalse(BaseMap.flatBuildings("fill", "park"))
        assertFalse(BaseMap.flatBuildings("fill-extrusion", "building"))
    }
}
