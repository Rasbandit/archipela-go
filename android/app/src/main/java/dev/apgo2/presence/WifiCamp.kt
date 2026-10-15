package dev.apgo2.presence

/** Which camp the Wi-Fi setup step saves networks for. Pure. */
internal object WifiCamp {
    /**
     * The camp selected when the step opens: [preselect] (the camp whose Wi-Fi button opened it) while it is still a camp, else the
     * camp the core finds near the phone ([near], `Engine.nearestCamp`), else the first camp; `null` only with no camps.
     */
    fun pick(
        campIds: List<String>,
        preselect: String?,
        near: String?,
    ): String? = listOfNotNull(preselect, near).firstOrNull { it in campIds } ?: campIds.firstOrNull()

    /** The saved networks as camp [campId] sees them: only those saved for it are ticked. */
    fun savedFor(
        saved: List<HomeNetwork>,
        campId: String?,
    ): List<HomeNetwork> = saved.filter { it.campId == campId }
}
