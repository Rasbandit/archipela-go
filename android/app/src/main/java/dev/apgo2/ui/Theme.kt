package dev.apgo2.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

private val P = ApgoPalette

private val LightColors =
    lightColorScheme(
        primary = P.teal,
        onPrimary = Color.White,
        primaryContainer = P.sky,
        onPrimaryContainer = Color(0xFF0B2A3A),
        secondary = Color(0xFF4F8A98),
        onSecondary = Color.White,
        secondaryContainer = P.mint,
        onSecondaryContainer = Color(0xFF0F2F36),
        tertiary = Color(0xFF8A6D00),
        onTertiary = Color.White,
        tertiaryContainer = P.butter,
        onTertiaryContainer = Color(0xFF3A3000),
        background = Color(0xFFF3FAFB),
        onBackground = P.navy,
        surface = Color(0xFFF3FAFB),
        onSurface = P.navy,
        surfaceVariant = P.mint,
        onSurfaceVariant = Color(0xFF2F4858),
        outline = Color(0xFF699CA8),
        outlineVariant = Color(0xFFB7D4D8),
        surfaceContainerLowest = Color.White,
        surfaceContainerLow = Color(0xFFF5FBFC),
        surfaceContainer = Color(0xFFEEF8F9),
        surfaceContainerHigh = Color(0xFFE9F5F7),
        surfaceContainerHighest = Color(0xFFE3F2F4),
    )

private val DarkColors =
    darkColorScheme(
        primary = P.sky,
        onPrimary = Color(0xFF0B2A3A),
        primaryContainer = P.teal,
        onPrimaryContainer = Color.White,
        secondary = P.butter,
        onSecondary = Color(0xFF3A3000),
        secondaryContainer = Color(0xFF52501F),
        onSecondaryContainer = P.butter,
        tertiary = P.grass,
        onTertiary = Color(0xFF003912),
        tertiaryContainer = Color(0xFF1F6B2A),
        onTertiaryContainer = Color(0xFFB5E9A4),
        background = Color(0xFF0E1C33),
        onBackground = Color(0xFFE6EEF7),
        surface = P.navy,
        onSurface = Color(0xFFE6EEF7),
        surfaceVariant = Color(0xFF1F3A5C),
        onSurfaceVariant = Color(0xFFB8CCE0),
        outline = Color(0xFF83A8E1),
        outlineVariant = Color(0xFF4C658B),
        surfaceContainerLowest = Color(0xFF0B1729),
        surfaceContainerLow = Color(0xFF142A44),
        surfaceContainer = Color(0xFF183049),
        surfaceContainerHigh = Color(0xFF1D3556),
        surfaceContainerHighest = Color(0xFF223C61),
    )

/** The one place the app picks its look. Screens use MaterialTheme roles and the components in Components.kt, never raw colours. */
@Composable
fun ApgoTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = if (isSystemInDarkTheme()) DarkColors else LightColors, content = content)
}
