package com.easydownload

/** Pending Android shares are retained until Flutter acknowledges that exact item. */
internal class PendingSharedContentQueue(private val maxPending: Int = 20) {
    private var nextId = 1L
    private val pending = linkedMapOf<Long, String>()

    @Synchronized
    fun capture(action: String?, text: String?): PendingSharedContent? {
        if (action != ACTION_SEND) return null
        val normalized = text?.trim()?.takeIf { it.isNotEmpty() } ?: return null
        val payload = PendingSharedContent(nextId++, normalized)
        pending[payload.id] = payload.text
        while (pending.size > maxPending.coerceAtLeast(1)) pending.remove(pending.keys.first())
        return payload
    }

    @Synchronized
    fun snapshot(): List<PendingSharedContent> =
        pending.map { (id, text) -> PendingSharedContent(id, text) }

    @Synchronized
    fun acknowledge(id: Long) { pending.remove(id) }

    companion object { private const val ACTION_SEND = "android.intent.action.SEND" }
}

internal data class PendingSharedContent(val id: Long, val text: String) {
    fun toChannelMap(): Map<String, Any> = mapOf("id" to id, "text" to text)
}
