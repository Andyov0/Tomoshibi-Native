package app.tomoshibi.ui.kit

import android.content.ClipData
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.Crossfade
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.tomoshibi.basics.Theme
import app.tomoshibi.basics.t
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

// Fields (Kit+Settings.swift's GlassField, SealControl and CopyLinkButton;
// Kit+Media.swift's MediaField). On Android a field takes no capital, no
// correction and no suggestion, as the Swift ones ask of the iPhone's
// keyboard: what is typed here is a name, a room, a passphrase or an
// address, and a keyboard that "corrects" a room name joins the wrong room.

/**
 * A field on the app's dark ground: a symbol that lights while there is
 * something in it, and for a secret an eye that shows it once there is
 * something to show (Identity.tsx: "an eye beside an empty field is a control
 * offering to reveal nothing") (`GlassField`). `lights` wears the person's
 * colour once the field holds something: the web lights the key beside a
 * passphrase because a name nobody can take is the one thing on that screen
 * worth lighting.
 */
@Composable
fun GlassField(
    prompt: String,
    text: String,
    onTextChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    symbol: String? = null,
    lights: Boolean = false,
    secret: Boolean = false,
    mono: Boolean = false,
    readOnly: Boolean = false,
    height: Dp = 44.dp,
    keyboardType: KeyboardType = KeyboardType.Text,
    onSubmit: () -> Unit = {},
) {
    var shown by rememberSaveable { mutableStateOf(false) }
    val symbolInk by animateColorAsState(if (lights && text.isNotEmpty()) Theme.tallyInk else Theme.muted, Theme.settle(), label = "symbol")
    val shape = rounded(14f)
    val font = if (mono) Font(14f, mono = true) else Font(14f)
    Row(
        modifier
            .fillMaxWidth()
            .height(height)
            .background(Theme.recess(0.24f), shape)
            .border(1.dp, Theme.wash.copy(alpha = 0.10f), shape)
            .padding(horizontal = 14.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (symbol != null) {
            Box(Modifier.width(18.dp), contentAlignment = Alignment.Center) {
                Symbol(symbol, size = 14f, weight = Font.Medium, tint = symbolInk)
            }
        }
        Box(Modifier.weight(1f), contentAlignment = Alignment.CenterStart) {
            if (readOnly) {
                SelectionContainer { KitText(text, font = font, color = Theme.muted, lines = 1) }
            } else {
                BasicTextField(
                    value = text,
                    onValueChange = onTextChange,
                    modifier = Modifier.fillMaxWidth(),
                    textStyle = font.style(Theme.foreground),
                    singleLine = true,
                    cursorBrush = SolidColor(Theme.tallyInk),
                    visualTransformation = if (secret && !shown) PasswordVisualTransformation() else VisualTransformation.None,
                    keyboardOptions = KeyboardOptions(
                        capitalization = KeyboardCapitalization.None,
                        autoCorrectEnabled = false,
                        keyboardType = if (secret && !shown) KeyboardType.Password else keyboardType,
                        imeAction = ImeAction.Done,
                    ),
                    keyboardActions = KeyboardActions(onDone = { onSubmit() }),
                    decorationBox = { field ->
                        Box(contentAlignment = Alignment.CenterStart) {
                            if (text.isEmpty()) KitText(prompt, font = font, color = Theme.muted, lines = 1, fit = true)
                            field()
                        }
                    },
                )
            }
        }
        AnimatedVisibility(secret && text.isNotEmpty(), enter = Theme.arrive, exit = Theme.depart) {
            val press = rememberPress()
            Box(
                Modifier
                    .size(24.dp)
                    .pressable(press, Capsule, true, Role.Button, if (shown) t("Hide passphrase") else t("Show passphrase")) {
                        shown = !shown
                    },
                contentAlignment = Alignment.Center,
            ) { SwappingSymbol(if (shown) "eye.slash" else "eye", size = 13f, tint = Theme.muted) }
        }
    }
}

/**
 * Sealed.tsx: a switch that opens onto one field and one sentence
 * (`SealControl`). The word and the room name make the key; everybody who
 * types the same word hears each other and the relay does not. The title
 * says "This call is sealed" once a word is typed, in the person's colour,
 * which is the only promise on the screen about who can read the call.
 * Turned off, the word is forgotten.
 */
@Composable
fun SealControl(
    on: Boolean,
    onOnChange: (Boolean) -> Unit,
    word: String,
    onWordChange: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val sealed = on && word.isNotEmpty()
    val ink by animateColorAsState(if (sealed) Theme.tallyInk else Theme.foreground, Theme.settle(), label = "title")
    val lock by animateColorAsState(if (sealed) Theme.tallyInk else Theme.muted, Theme.settle(), label = "lock")
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.width(18.dp), contentAlignment = Alignment.Center) {
                SwappingSymbol(if (sealed) "lock.fill" else "lock", size = 13f, weight = Font.Semibold, tint = lock)
            }
            Crossfade(sealed, Modifier.weight(1f), animationSpec = Theme.settle(), label = "seal") { now ->
                KitText(
                    if (now) t("This call is sealed") else t("Seal this call"),
                    font = Font(14f, if (now) Font.Semibold else Font.Regular),
                    color = ink,
                )
            }
            GlassSwitch(
                on,
                { turned ->
                    onOnChange(turned)
                    if (!turned) onWordChange("")
                },
                Modifier.padding(start = 18.dp),
                label = t("Seal this call"),
            )
        }
        AnimatedVisibility(on, enter = Theme.arrive, exit = Theme.depart) {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                GlassField(t("Sealing word"), word, onWordChange, symbol = "key", lights = true, secret = true, height = 40.dp)
                KitText(
                    t("Everybody who types the same word can hear each other, and the server carrying the call cannot. Tell people the word some other way."),
                    font = Font(11.5f),
                    color = Theme.muted,
                )
            }
        }
    }
}

/** Copies a link and says so for a moment, as the web's copy buttons do (`CopyLinkButton`). */
@Composable
fun CopyLinkButton(link: String, modifier: Modifier = Modifier, size: Dp = 32.dp) {
    var copied by remember { mutableStateOf(false) }
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    LaunchedEffect(copied) {
        if (copied) {
            delay(1600)
            copied = false
        }
    }
    val ink by animateColorAsState(if (copied) Theme.tallyInk else Theme.foreground, Theme.settle(), label = "ink")
    GlassButton(
        onClick = {
            scope.launch { clipboard.setClipEntry(ClipEntry(ClipData.newPlainText(t("Copy link"), link))) }
            copied = true
        },
        modifier = modifier,
        shape = GlassButtonShape.Circle(size),
        label = if (copied) t("Link copied") else t("Copy link"),
    ) {
        SwappingSymbol(if (copied) "checkmark" else "doc.on.doc", size = size.value * 0.38f, weight = Font.Semibold, tint = ink)
    }
}

/**
 * A field in a capsule of dark glass, whose edge takes the chosen colour
 * while it is being typed in (`MediaField`). `limit` holds it to as many
 * UTF-16 units as the web's field takes (`maxLength`), never cutting a
 * character in two. `url` asks for the keyboard made for addresses.
 */
@Composable
fun MediaField(
    prompt: String,
    text: String,
    onTextChange: (String) -> Unit,
    onSubmit: () -> Unit,
    modifier: Modifier = Modifier,
    symbol: String? = null,
    limit: Int? = null,
    url: Boolean = false,
) {
    val source = remember { MutableInteractionSource() }
    val focused by source.collectIsFocusedAsState()
    val focus = remember { FocusRequester() }
    val edge by animateColorAsState(if (focused) Theme.tallyInk.copy(alpha = 0.75f) else Theme.wash.copy(alpha = 0.09f), Theme.settle(), label = "edge")
    val width by animateDpAsState(if (focused) 1.5.dp else 1.dp, Theme.settle(), label = "width")
    val symbolInk by animateColorAsState(if (focused) Theme.foreground else Theme.muted, Theme.settle(), label = "symbol")
    val font = Font(13f)
    Row(
        modifier
            .fillMaxWidth()
            .height(34.dp)
            .background(Theme.recess(0.24f), Capsule)
            .border(width, edge, Capsule)
            .clickable(interactionSource = null, indication = null) { focus.requestFocus() }
            .padding(horizontal = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (symbol != null) Symbol(symbol, size = 12f, weight = Font.Semibold, tint = symbolInk)
        BasicTextField(
            value = text,
            onValueChange = { typed -> onTextChange(if (limit != null) clampUnits(typed, limit) else typed) },
            modifier = Modifier.weight(1f).focusRequester(focus),
            textStyle = font.style(Theme.foreground),
            singleLine = true,
            cursorBrush = SolidColor(Theme.tallyInk),
            interactionSource = source,
            keyboardOptions = KeyboardOptions(
                capitalization = KeyboardCapitalization.None,
                autoCorrectEnabled = false,
                keyboardType = if (url) KeyboardType.Uri else KeyboardType.Text,
                imeAction = ImeAction.Go,
            ),
            keyboardActions = KeyboardActions(onGo = { onSubmit() }),
            decorationBox = { field ->
                Box(contentAlignment = Alignment.CenterStart) {
                    if (text.isEmpty()) KitText(prompt, font = font, color = Theme.muted, lines = 1, fit = true)
                    field()
                }
            },
        )
    }
}

/**
 * `text` cut to `limit` UTF-16 units, as a browser counts `maxLength`; a
 * pair of surrogates the limit falls between is left out whole rather than
 * kept as half a character.
 */
fun clampUnits(text: String, limit: Int): String {
    if (text.length <= limit) return text
    val cut = text.substring(0, limit)
    return if (cut.isNotEmpty() && cut.last().isHighSurrogate()) cut.dropLast(1) else cut
}
