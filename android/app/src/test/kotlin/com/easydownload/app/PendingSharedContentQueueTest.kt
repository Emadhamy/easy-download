package com.easydownload

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PendingSharedContentQueueTest {
    @Test fun acceptsOnlyNonEmptyTextSendIntents() {
        val queue = PendingSharedContentQueue()
        assertNull(queue.capture("android.intent.action.VIEW", "https://example.com/video"))
        assertNull(queue.capture("android.intent.action.SEND", "   "))
        assertEquals(emptyList<PendingSharedContent>(), queue.snapshot())
        val payload = queue.capture("android.intent.action.SEND", "  https://example.com/video  ")
        assertEquals("https://example.com/video", payload?.text)
        assertEquals(1L, payload?.id)
    }

    @Test fun acknowledgesOnlyTheMatchingShareAndKeepsNewerShares() {
        val queue = PendingSharedContentQueue()
        val first = queue.capture("android.intent.action.SEND", "one")!!
        val second = queue.capture("android.intent.action.SEND", "two")!!
        queue.acknowledge(first.id)
        assertEquals(listOf(second), queue.snapshot())
    }

    @Test fun retainsPendingSharesUntilAcknowledgedAndBoundsTheQueue() {
        val queue = PendingSharedContentQueue(maxPending = 2)
        val first = queue.capture("android.intent.action.SEND", "one")!!
        val second = queue.capture("android.intent.action.SEND", "two")!!
        val third = queue.capture("android.intent.action.SEND", "three")!!
        assertEquals(listOf(second, third), queue.snapshot())
        queue.acknowledge(third.id)
        assertEquals(listOf(second), queue.snapshot())
        queue.acknowledge(first.id)
        assertEquals(listOf(second), queue.snapshot())
    }
}
