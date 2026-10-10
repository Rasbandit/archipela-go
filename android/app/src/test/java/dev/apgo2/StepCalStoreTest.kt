package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.apgo_ffi.StepCalOut

class StepCalStoreTest {
    @Test fun aCalibrationRoundTripsUnderItsSource() {
        val text = StepCalCodec.encode(StepCalOut("phone.step_counter", 0.92, 0.0004, 17u, 1_800_000_000_000L))
        val back = StepCalCodec.decode("phone.step_counter", text)!!
        assertEquals("phone.step_counter", back.source)
        assertEquals(0.92, back.k, 1e-12)
        assertEquals(0.0004, back.varK, 1e-12)
        assertEquals(17u, back.samples)
        assertEquals(1_800_000_000_000L, back.updatedMs)
    }

    @Test fun nothingStoredOrGarbageIsNull() {
        assertNull(StepCalCodec.decode("phone.step_counter", null))
        assertNull(StepCalCodec.decode("phone.step_counter", "1.0;x"))
        assertNull(StepCalCodec.decode("phone.step_counter", "a;b;c;d"))
        assertNull(StepCalCodec.decode("phone.step_counter", "NaN;0.01;3;5"))
        assertNull(StepCalCodec.decode("phone.step_counter", "Infinity;0.01;3;5"))
        assertNull(StepCalCodec.decode("phone.step_counter", "1.0;NaN;3;5"))
        assertNull(StepCalCodec.decode("phone.step_counter", "1.0;0.0;3;5"))
    }
}
