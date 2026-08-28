package com.meshos.mobile

import android.content.Context
import android.net.Uri
import android.os.Bundle
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject

private object MeshNative {
    init {
        System.loadLibrary("mesh_core")
    }

    external fun version(): String
    external fun ping(): String
    external fun startReceiver(deviceName: String): String
    external fun discover(ownId: String): String
    external fun pair(address: String, deviceName: String): String
    external fun sendFile(
        address: String,
        deviceName: String,
        localPath: String,
        remotePath: String
    ): String
}

private val Bg = Color(0xFF070B12)
private val Surface = Color(0xFF101824)
private val Surface2 = Color(0xFF162235)
private val Accent = Color(0xFF4B8DFF)
private val Success = Color(0xFF42E39A)
private val Danger = Color(0xFFFF6574)
private val Muted = Color(0xFF94A3B8)

private enum class Screen {
    HOME, FILES, MESH, TRANSFERS, CONFLICTS, SETTINGS
}

private data class Device(
    val id: String = "",
    val name: String,
    val address: String,
    val online: Boolean,
    val trusted: Boolean
)

private data class Transfer(
    val file: String,
    val size: String,
    val status: String,
    val progress: Int
)

private data class UiState(
    val connected: Boolean = true,
    val devices: List<Device> = emptyList(),
    val transfers: List<Transfer> = emptyList(),
    val conflicts: Int = 0
)

private class TrustedStore(
    context: Context
) {
    private val prefs =
        context.getSharedPreferences(
            "meshos_trusted",
            Context.MODE_PRIVATE
        )

    fun isTrusted(address: String): Boolean =
        prefs.getBoolean(address, false)

    fun setTrusted(address: String, value: Boolean) {
        prefs.edit().putBoolean(address, value).apply()
    }
}

private fun copyUriToCache(
    context: Context,
    uri: Uri,
    name: String
): String {
    val safe =
        name.replace(
            Regex("[^A-Za-z0-9._-]"),
            "_"
        )

    val out =
        java.io.File(
            context.cacheDir,
            safe
        )

    context.contentResolver
        .openInputStream(uri)
        .use { input ->
            requireNotNull(input)

            java.io.FileOutputStream(out)
                .use { output ->
                    input.copyTo(output)
                }
        }

    return out.absolutePath
}

private fun displayName(
    context: Context,
    uri: Uri
): String {
    var value = ""

    context.contentResolver
        .query(
            uri,
            arrayOf(
                android.provider.OpenableColumns.DISPLAY_NAME
            ),
            null,
            null,
            null
        )
        ?.use { cursor ->
            if (cursor.moveToFirst()) {
                val index =
                    cursor.getColumnIndex(
                        android.provider.OpenableColumns.DISPLAY_NAME
                    )

                if (index >= 0) {
                    value = cursor.getString(index)
                }
            }
        }

    return if (value.isBlank())
        "MeshOS-file"
    else
        value
}

class MainActivity : ComponentActivity() {

    override fun onCreate(
        savedInstanceState: Bundle?
    ) {
        super.onCreate(savedInstanceState)

        MeshNative.startReceiver(
            "MeshOS Phone"
        )

        setContent {
            MeshOSApp()
        }
    }
}

@Composable
private fun MeshOSApp() {
    var screen by remember {
        mutableStateOf(Screen.HOME)
    }

    var state by remember {
        mutableStateOf(UiState())
    }

    MaterialTheme(
        colorScheme = darkColorScheme(
            primary = Accent,
            background = Bg,
            surface = Surface
        )
    ) {
        Scaffold(
            containerColor = Bg,
            bottomBar = {
                NavigationBar(
                    containerColor =
                        Color(0xFF0B111A)
                ) {
                    listOf(
                        Screen.HOME to "Home",
                        Screen.FILES to "Files",
                        Screen.MESH to "Mesh",
                        Screen.TRANSFERS to "Transfers",
                        Screen.CONFLICTS to "Conflicts",
                        Screen.SETTINGS to "Settings"
                    ).forEach { (target, label) ->
                        NavigationBarItem(
                            selected =
                                screen == target,
                            onClick = {
                                screen = target
                            },
                            icon = {
                                Text(
                                    when (target) {
                                        Screen.HOME -> "⌂"
                                        Screen.FILES -> "▣"
                                        Screen.MESH -> "◈"
                                        Screen.TRANSFERS -> "⇅"
                                        Screen.CONFLICTS -> "⚠"
                                        Screen.SETTINGS -> "⚙"
                                    }
                                )
                            },
                            label = {
                                Text(label)
                            }
                        )
                    }
                }
            }
        ) { padding ->
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(padding)
            ) {
                when (screen) {
                    Screen.HOME ->
                        HomeScreen(
                            state = state,
                            onNavigate = {
                                screen = it
                            }
                        )

                    Screen.MESH ->
                        MeshScreen(
                            state = state,
                            onDevices = {
                                state =
                                    state.copy(
                                        devices = it
                                    )
                            }
                        )

                    Screen.FILES ->
                        FilesScreen(
                            state = state,
                            onTransfer = {
                                state =
                                    state.copy(
                                        transfers =
                                            state.transfers + it
                                    )
                                screen =
                                    Screen.TRANSFERS
                            }
                        )

                    Screen.TRANSFERS ->
                        TransfersScreen(
                            transfers =
                                state.transfers
                        )

                    Screen.CONFLICTS ->
                        ConflictsScreen(
                            count =
                                state.conflicts
                        )

                    Screen.SETTINGS ->
                        SettingsScreen(
                            connected =
                                state.connected,
                            onToggle = {
                                state =
                                    state.copy(
                                        connected =
                                            !state.connected
                                    )
                            }
                        )
                }
            }
        }
    }
}

@Composable
private fun HomeScreen(
    state: UiState,
    onNavigate: (Screen) -> Unit
) {
    LazyColumn(
        modifier = Modifier
            .fillMaxSize()
            .background(Bg),
        contentPadding =
            PaddingValues(20.dp),
        verticalArrangement =
            Arrangement.spacedBy(14.dp)
    ) {
        item {
            TopBar(
                "Home",
                state.connected
            )
        }

        item {
            Text(
                "MeshOS Mobile",
                color = Color.White,
                fontSize = 32.sp,
                fontWeight = FontWeight.ExtraBold
            )
        }

        item {
            Text(
                "Private • Secure • Offline-first",
                color = Muted,
                fontSize = 15.sp
            )
        }

        item {
            Row(
                horizontalArrangement =
                    Arrangement.spacedBy(12.dp),
                modifier =
                    Modifier.fillMaxWidth()
            ) {
                FeatureCard(
                    title = "Files",
                    subtitle = "Send & receive",
                    icon = "▣",
                    modifier =
                        Modifier.weight(1f),
                    onClick = {
                        onNavigate(Screen.FILES)
                    }
                )

                FeatureCard(
                    title = "Mesh",
                    subtitle =
                        "${state.devices.count { it.online }} online",
                    icon = "◈",
                    modifier =
                        Modifier.weight(1f),
                    onClick = {
                        onNavigate(Screen.MESH)
                    }
                )
            }
        }

        item {
            Row(
                horizontalArrangement =
                    Arrangement.spacedBy(12.dp),
                modifier =
                    Modifier.fillMaxWidth()
            ) {
                FeatureCard(
                    title = "Transfers",
                    subtitle =
                        "${state.transfers.size} records",
                    icon = "⇅",
                    modifier =
                        Modifier.weight(1f),
                    onClick = {
                        onNavigate(
                            Screen.TRANSFERS
                        )
                    }
                )

                FeatureCard(
                    title = "Conflicts",
                    subtitle =
                        "${state.conflicts} unresolved",
                    icon = "⚠",
                    modifier =
                        Modifier.weight(1f),
                    onClick = {
                        onNavigate(
                            Screen.CONFLICTS
                        )
                    }
                )
            }
        }

        item {
            SystemCard(
                "Security",
                "Ed25519 • X25519 • HKDF • encrypted channel"
            )
        }

        item {
            SystemCard(
                "Storage",
                "Persistent trusted devices and received files"
            )
        }

        item {
            SystemCard(
                "Backend",
                MeshNative.version()
            )
        }
    }
}

@Composable
private fun TopBar(
    title: String,
    connected: Boolean
) {
    Column(
        modifier = Modifier.fillMaxWidth()
    ) {
        Text(
            "MeshOS",
            color = Accent,
            fontSize = 13.sp,
            fontWeight = FontWeight.Bold
        )

        Text(
            title,
            color = Color.White,
            fontSize = 28.sp,
            fontWeight = FontWeight.ExtraBold
        )

        Spacer(Modifier.height(6.dp))

        Text(
            if (connected)
                "● Online"
            else
                "● Offline",
            color =
                if (connected)
                    Success
                else
                    Danger,
            fontSize = 13.sp
        )

        Spacer(Modifier.height(18.dp))
    }
}

@Composable
private fun FeatureCard(
    title: String,
    subtitle: String,
    icon: String,
    modifier: Modifier,
    onClick: () -> Unit
) {
    Card(
        modifier =
            modifier.clickable(
                onClick = onClick
            ),
        shape =
            RoundedCornerShape(22.dp),
        colors =
            CardDefaults.cardColors(
                containerColor = Surface
            )
    ) {
        Column(
            modifier =
                Modifier.padding(18.dp)
        ) {
            Text(
                icon,
                color = Accent,
                fontSize = 28.sp
            )

            Spacer(Modifier.height(12.dp))

            Text(
                title,
                color = Color.White,
                fontSize = 18.sp,
                fontWeight = FontWeight.Bold
            )

            Text(
                subtitle,
                color = Muted,
                fontSize = 13.sp
            )
        }
    }
}

@Composable
private fun SystemCard(
    title: String,
    value: String
) {
    Card(
        modifier =
            Modifier.fillMaxWidth(),
        shape =
            RoundedCornerShape(18.dp),
        colors =
            CardDefaults.cardColors(
                containerColor = Surface
            )
    ) {
        Column(
            modifier =
                Modifier.padding(18.dp)
        ) {
            Text(
                title,
                color = Color.White,
                fontWeight = FontWeight.Bold
            )

            Spacer(Modifier.height(6.dp))

            Text(
                value,
                color = Muted,
                fontSize = 13.sp
            )
        }
    }
}

@Composable
private fun MeshScreen(
    state: UiState,
    onDevices: (List<Device>) -> Unit
) {
    val context =
        LocalContext.current

    val trustedStore =
        remember {
            TrustedStore(context)
        }

    val ownId =
        remember {
            Settings.Secure.getString(
                context.contentResolver,
                Settings.Secure.ANDROID_ID
            ) ?: "android-device"
        }

    val scope =
        rememberCoroutineScope()

    var scanning by remember {
        mutableStateOf(false)
    }

    var pairingAddress by remember {
        mutableStateOf<String?>(null)
    }

    var message by remember {
        mutableStateOf("")
    }

    fun scan() {
        scanning = true
        message = ""

        scope.launch {
            val raw =
                withContext(Dispatchers.IO) {
                    MeshNative.discover(
                        ownId
                    )
                }

            runCatching {
                val array =
                    JSONArray(raw)

                buildList {
                    for (i in 0 until array.length()) {
                        val o =
                            array.getJSONObject(i)

                        val address =
                            o.optString(
                                "address"
                            )

                        add(
                            Device(
                                id =
                                    o.optString(
                                        "id"
                                    ),
                                name =
                                    o.optString(
                                        "name",
                                        "MeshOS Device"
                                    ),
                                address =
                                    address,
                                online =
                                    o.optBoolean(
                                        "online",
                                        true
                                    ),
                                trusted =
                                    trustedStore.isTrusted(
                                        address
                                    )
                            )
                        )
                    }
                }
            }.onSuccess {
                onDevices(it)

                message =
                    if (it.isEmpty())
                        "No MeshOS devices found."
                    else
                        "${it.size} device(s) found."
            }.onFailure {
                message =
                    "Discovery failed: ${it.message}"
            }

            scanning = false
        }
    }

    Column(
        modifier =
            Modifier
                .fillMaxSize()
                .background(Bg)
                .padding(20.dp)
    ) {
        TopBar(
            "Mesh Devices",
            state.connected
        )

        Row(
            modifier =
                Modifier.fillMaxWidth(),
            verticalAlignment =
                Alignment.CenterVertically
        ) {
            Column(
                modifier =
                    Modifier.weight(1f)
            ) {
                Text(
                    "${state.devices.size} devices",
                    color = Color.White,
                    fontWeight = FontWeight.Bold
                )

                Text(
                    "UDP discovery • TCP 45873 secure pairing",
                    color = Muted,
                    fontSize = 12.sp
                )
            }

            Button(
                enabled =
                    !scanning &&
                    pairingAddress == null,
                onClick = {
                    scan()
                }
            ) {
                Text(
                    if (scanning)
                        "Scanning..."
                    else
                        "Scan"
                )
            }
        }

        if (message.isNotBlank()) {
            Spacer(Modifier.height(12.dp))

            Text(
                message,
                color = Color.White
            )
        }

        Spacer(Modifier.height(12.dp))

        LazyColumn(
            verticalArrangement =
                Arrangement.spacedBy(10.dp)
        ) {
            items(
                items = state.devices,
                key = {
                    it.id.ifBlank {
                        it.address
                    }
                }
            ) { device ->

                Card(
                    modifier =
                        Modifier.fillMaxWidth(),
                    shape =
                        RoundedCornerShape(18.dp),
                    colors =
                        CardDefaults.cardColors(
                            containerColor =
                                Surface
                        )
                ) {
                    Row(
                        modifier =
                            Modifier
                                .fillMaxWidth()
                                .padding(18.dp),
                        verticalAlignment =
                            Alignment.CenterVertically
                    ) {
                        Column(
                            modifier =
                                Modifier.weight(1f)
                        ) {
                            Text(
                                device.name,
                                color =
                                    Color.White,
                                fontWeight =
                                    FontWeight.Bold
                            )

                            Text(
                                device.address,
                                color = Muted,
                                fontSize = 12.sp
                            )

                            Text(
                                if (device.trusted)
                                    "Trusted"
                                else
                                    "Pairing required",
                                color =
                                    if (device.trusted)
                                        Success
                                    else
                                        Muted,
                                fontSize = 12.sp
                            )
                        }

                        if (device.trusted) {
                            Text(
                                "✓",
                                color = Success,
                                fontSize = 24.sp
                            )
                        } else {
                            Button(
                                enabled =
                                    pairingAddress == null,
                                onClick = {
                                    pairingAddress =
                                        device.address
                                    message =
                                        "Pairing with ${device.name}..."

                                    scope.launch {
                                        val raw =
                                            withContext(
                                                Dispatchers.IO
                                            ) {
                                                MeshNative.pair(
                                                    device.address,
                                                    "MeshOS Phone"
                                                )
                                            }

                                        if (
                                            raw.contains(
                                                "\"ok\":true"
                                            )
                                        ) {
                                            trustedStore
                                                .setTrusted(
                                                    device.address,
                                                    true
                                                )

                                            message =
                                                "✅ ${device.name} trusted."

                                            onDevices(
                                                state.devices.map {
                                                    if (
                                                        it.address ==
                                                        device.address
                                                    ) {
                                                        it.copy(
                                                            trusted =
                                                                true
                                                        )
                                                    } else {
                                                        it
                                                    }
                                                }
                                            )
                                        } else {
                                            message =
                                                raw
                                        }

                                        pairingAddress =
                                            null
                                    }
                                }
                            ) {
                                Text(
                                    if (
                                        pairingAddress ==
                                        device.address
                                    )
                                        "Pairing..."
                                    else
                                        "Pair"
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun FilesScreen(
    state: UiState,
    onTransfer: (Transfer) -> Unit
) {
    val context =
        LocalContext.current

    val scope =
        rememberCoroutineScope()

    val target =
        state.devices.firstOrNull {
            it.trusted && it.online
        }

    var selectedUri by remember {
        mutableStateOf<Uri?>(null)
    }

    var selectedName by remember {
        mutableStateOf("")
    }

    var sending by remember {
        mutableStateOf(false)
    }

    var message by remember {
        mutableStateOf("")
    }

    val picker =
        rememberLauncherForActivityResult(
            ActivityResultContracts.OpenDocument()
        ) { uri: Uri? ->
            selectedUri = uri

            if (uri != null) {
                selectedName =
                    displayName(
                        context,
                        uri
                    )
            }
        }

    Column(
        modifier =
            Modifier
                .fillMaxSize()
                .background(Bg)
                .padding(20.dp)
    ) {
        TopBar(
            "Files",
            state.connected
        )

        Text(
            "Send any selected file through MeshOS.",
            color = Muted
        )

        Spacer(Modifier.height(16.dp))

        Text(
            "Target: " +
                (
                    target?.name
                        ?: "No trusted device"
                ),
            color = Color.White,
            fontWeight = FontWeight.Bold
        )

        Spacer(Modifier.height(14.dp))

        Card(
            modifier =
                Modifier.fillMaxWidth(),
            shape =
                RoundedCornerShape(18.dp),
            colors =
                CardDefaults.cardColors(
                    containerColor = Surface
                )
        ) {
            Column(
                modifier =
                    Modifier.padding(18.dp)
            ) {
                Text(
                    if (
                        selectedName.isBlank()
                    )
                        "No file selected"
                    else
                        selectedName,
                    color = Color.White,
                    fontWeight = FontWeight.Bold
                )

                Spacer(Modifier.height(14.dp))

                Row(
                    horizontalArrangement =
                        Arrangement.spacedBy(10.dp)
                ) {
                    OutlinedButton(
                        enabled = !sending,
                        onClick = {
                            picker.launch(
                                arrayOf("*/*")
                            )
                        }
                    ) {
                        Text("Choose file")
                    }

                    Button(
                        enabled =
                            !sending &&
                            selectedUri != null &&
                            target != null,
                        onClick = {
                            val uri =
                                selectedUri
                                    ?: return@Button

                            val device =
                                target
                                    ?: return@Button

                            sending = true
                            message =
                                "Preparing transfer..."

                            scope.launch {
                                try {
                                    val cached =
                                        withContext(
                                            Dispatchers.IO
                                        ) {
                                            copyUriToCache(
                                                context,
                                                uri,
                                                selectedName
                                            )
                                        }

                                    val raw =
                                        withContext(
                                            Dispatchers.IO
                                        ) {
                                            MeshNative.sendFile(
                                                device.address,
                                                "MeshOS Phone",
                                                cached,
                                                selectedName
                                            )
                                        }

                                    if (
                                        raw.contains(
                                            "\"ok\":true"
                                        )
                                    ) {
                                        message =
                                            "✅ File sent successfully."

                                        onTransfer(
                                            Transfer(
                                                file =
                                                    selectedName,
                                                size =
                                                    "Completed",
                                                status =
                                                    "Completed",
                                                progress =
                                                    100
                                            )
                                        )
                                    } else {
                                        message = raw
                                    }
                                } catch (
                                    e: Exception
                                ) {
                                    message =
                                        "❌ ${e.message}"
                                } finally {
                                    sending =
                                        false
                                }
                            }
                        }
                    ) {
                        Text(
                            if (sending)
                                "Sending..."
                            else
                                "Send"
                        )
                    }
                }

                if (
                    message.isNotBlank()
                ) {
                    Spacer(Modifier.height(14.dp))

                    Text(
                        message,
                        color =
                            if (
                                message.startsWith(
                                    "✅"
                                )
                            )
                                Success
                            else
                                Color.White
                    )
                }
            }
        }
    }
}

@Composable
private fun TransfersScreen(
    transfers: List<Transfer>
) {
    LazyColumn(
        modifier =
            Modifier
                .fillMaxSize()
                .background(Bg),
        contentPadding =
            PaddingValues(20.dp),
        verticalArrangement =
            Arrangement.spacedBy(10.dp)
    ) {
        item {
            TopBar(
                "Transfers",
                true
            )
        }

        if (
            transfers.isEmpty()
        ) {
            item {
                Text(
                    "No transfers yet.",
                    color = Muted
                )
            }
        }

        items(
            items = transfers
        ) { transfer ->
            Card(
                modifier =
                    Modifier.fillMaxWidth(),
                shape =
                    RoundedCornerShape(18.dp),
                colors =
                    CardDefaults.cardColors(
                        containerColor =
                            Surface
                    )
            ) {
                Column(
                    modifier =
                        Modifier.padding(18.dp)
                ) {
                    Text(
                        transfer.file,
                        color = Color.White,
                        fontWeight =
                            FontWeight.Bold
                    )

                    Spacer(
                        Modifier.height(5.dp)
                    )

                    Text(
                        "${transfer.size} • ${transfer.status}",
                        color = Muted,
                        fontSize = 13.sp
                    )

                    Spacer(
                        Modifier.height(10.dp)
                    )

                    LinearProgressIndicator(
                        progress = {
                            transfer.progress /
                                100f
                        },
                        modifier =
                            Modifier.fillMaxWidth()
                    )
                }
            }
        }
    }
}

@Composable
private fun ConflictsScreen(
    count: Int
) {
    Column(
        modifier =
            Modifier
                .fillMaxSize()
                .background(Bg)
                .padding(20.dp)
    ) {
        TopBar(
            "Conflicts",
            true
        )

        Text(
            if (count == 0)
                "No unresolved conflicts"
            else
                "$count conflict(s) require attention",
            color =
                if (count == 0)
                    Success
                else
                    Color.White,
            fontSize = 18.sp,
            fontWeight = FontWeight.Bold
        )

        Spacer(
            Modifier.height(10.dp)
        )

        Text(
            "Incoming files with the same name are preserved as conflict copies.",
            color = Muted,
            fontSize = 13.sp
        )
    }
}

@Composable
private fun SettingsScreen(
    connected: Boolean,
    onToggle: () -> Unit
) {
    LazyColumn(
        modifier =
            Modifier
                .fillMaxSize()
                .background(Bg),
        contentPadding =
            PaddingValues(20.dp),
        verticalArrangement =
            Arrangement.spacedBy(10.dp)
    ) {
        item {
            TopBar(
                "Settings",
                connected
            )
        }

        item {
            SystemCard(
                "Security",
                "Ed25519 identity • X25519 • HKDF • encrypted channel"
            )
        }

        item {
            SystemCard(
                "Storage",
                "Persistent trusted devices • received files • transfers"
            )
        }

        item {
            SystemCard(
                "Transfer",
                "32 KB chunks • SHA-256 • acknowledgement • conflict copy"
            )
        }

        item {
            Button(
                modifier =
                    Modifier.fillMaxWidth(),
                onClick = onToggle
            ) {
                Text(
                    if (connected)
                        "Simulate Offline"
                    else
                        "Reconnect"
                )
            }
        }
    }
}
