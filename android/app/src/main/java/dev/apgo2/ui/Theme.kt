package dev.apgo2.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable

/** The one place the app picks its look. Screens use MaterialTheme roles and the components in Components.kt, never raw colours. */
@Composable
internal fun ApgoTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = if (isSystemInDarkTheme()) ApgoPalette.darkScheme else ApgoPalette.lightScheme, content = content)
}
