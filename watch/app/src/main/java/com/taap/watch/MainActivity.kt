package com.taap.watch

import android.graphics.Bitmap
import android.graphics.Color
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.unit.dp
import androidx.wear.compose.material.Button
import androidx.wear.compose.material.CircularProgressIndicator
import androidx.wear.compose.material.Text
import com.google.zxing.BarcodeFormat
import com.google.zxing.qrcode.QRCodeWriter
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

// 화면 상태
private sealed interface UiState
private data object Idle : UiState
private data object Loading : UiState
private data class Success(val qr: Bitmap) : UiState
private data class Failure(val msg: String) : UiState

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { App() }
    }
}

@androidx.compose.runtime.Composable
private fun App() {
    var state by remember { mutableStateOf<UiState>(Idle) }
    val scope = rememberCoroutineScope()

    fun trigger() {
        state = Loading
        scope.launch {
            state = try {
                val serial = fetchCardSerial()
                Success(renderQr(serial, 400))
            } catch (e: Exception) {
                Failure(e.message ?: "오류")
            }
        }
    }

    Box(
        modifier = Modifier.fillMaxSize().background(androidx.compose.ui.graphics.Color.Black),
        contentAlignment = Alignment.Center,
    ) {
        when (val s = state) {
            is Idle -> Button(onClick = { trigger() }) { Text("QR 생성") }
            is Loading -> CircularProgressIndicator()
            is Success -> Column(horizontalAlignment = Alignment.CenterHorizontally) {
                // QR 은 흰 배경 위에 있어야 스캔된다
                Image(
                    bitmap = s.qr.asImageBitmap(),
                    contentDescription = "출입 QR",
                    modifier = Modifier
                        .size(150.dp)
                        .background(androidx.compose.ui.graphics.Color.White),
                )
                Spacer(Modifier.height(8.dp))
                Button(onClick = { trigger() }) { Text("새로고침") }
            }
            is Failure -> Column(horizontalAlignment = Alignment.CenterHorizontally) {
                Text("실패: ${s.msg}")
                Spacer(Modifier.height(8.dp))
                Button(onClick = { trigger() }) { Text("다시") }
            }
        }
    }
}

/** 백엔드 GET /qr → cardSerialNumber. */
private suspend fun fetchCardSerial(): String = withContext(Dispatchers.IO) {
    val conn = (URL("${BuildConfig.BACKEND_URL}/qr").openConnection() as HttpURLConnection).apply {
        requestMethod = "GET"
        connectTimeout = 10_000
        readTimeout = 15_000
    }
    try {
        if (conn.responseCode != 200) throw RuntimeException("서버 ${conn.responseCode}")
        val body = conn.inputStream.bufferedReader().use { it.readText() }
        JSONObject(body).getString("cardSerialNumber")
    } finally {
        conn.disconnect()
    }
}

/** 문자열을 QR 비트맵으로. */
private fun renderQr(text: String, size: Int): Bitmap {
    val matrix = QRCodeWriter().encode(text, BarcodeFormat.QR_CODE, size, size)
    val bmp = Bitmap.createBitmap(size, size, Bitmap.Config.RGB_565)
    for (x in 0 until size) for (y in 0 until size) {
        bmp.setPixel(x, y, if (matrix.get(x, y)) Color.BLACK else Color.WHITE)
    }
    return bmp
}
