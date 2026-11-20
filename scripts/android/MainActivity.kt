package com.seo1323.amicalculator

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.OpenableColumns
import android.view.View
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import org.json.JSONObject
import java.io.IOException
import kotlin.concurrent.thread

// Copied over the generated MainActivity by CI (and by hand for local builds, see PLAN.md).
// Android WebView reports env(safe-area-inset-*) for the display cutout only, so with edge-to-edge
// the page would sit under the status bar and the gesture bar. The insets are applied here as
// padding instead, and consumed so the page does not add them a second time.
//
// It also gives the page `window.AmiSync` for the sync file (src/lib/sync.svelte.ts): the dialog
// plugin hands out content:// URIs whose access ends with the app process, but sync must keep
// working after a restart, so the file is picked here with a persistable grant.
class MainActivity : TauriActivity() {
  private var web: WebView? = null
  private var pickId: Int? = null

  private val picker = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
    val id = pickId ?: return@registerForActivityResult
    pickId = null
    val uri = result.data?.data
    if (result.resultCode != Activity.RESULT_OK || uri == null) {
      reply(id, JSONObject().put("ok", true).put("uri", "")) // cancelled
      return@registerForActivityResult
    }
    reply(id, try {
      contentResolver.takePersistableUriPermission(
        uri, Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
      )
      JSONObject().put("ok", true).put("uri", uri.toString()).put("name", displayName(uri))
    } catch (e: Exception) {
      failure(e)
    })
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    val root = findViewById<View>(android.R.id.content)
    ViewCompat.setOnApplyWindowInsetsListener(root) { v, insets ->
      val bars = insets.getInsets(
        WindowInsetsCompat.Type.systemBars() or
          WindowInsetsCompat.Type.displayCutout() or
          WindowInsetsCompat.Type.ime()
      )
      v.setPadding(bars.left, bars.top, bars.right, bars.bottom)
      WindowInsetsCompat.CONSUMED
    }
  }

  override fun onWebViewCreate(webView: WebView) {
    web = webView
    webView.addJavascriptInterface(SyncBridge(), "AmiSync")
  }

  private fun failure(e: Throwable) = JSONObject().put("ok", false).put("error", e.message ?: e.toString())

  private fun reply(id: Int, result: JSONObject) {
    val js = "window.__amiSyncResult($id, ${JSONObject.quote(result.toString())})"
    web?.post { web?.evaluateJavascript(js, null) }
  }

  private fun displayName(uri: Uri): String =
    contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
      if (it.moveToFirst()) it.getString(0) else null
    } ?: uri.lastPathSegment ?: ""

  private fun content(uri: String): Uri {
    val u = Uri.parse(uri)
    require(u.scheme == "content") { "not a content:// URI" } // the page may only use what the user picked
    return u
  }

  private inner class SyncBridge {
    /** Answers through `window.__amiSyncResult(id, json)`; file access runs off the UI thread. */
    @JavascriptInterface
    fun call(id: Int, op: String, uri: String, text: String) {
      if (op == "pick") {
        runOnUiThread {
          pickId = id
          val intent =
            if (uri == "create") Intent(Intent.ACTION_CREATE_DOCUMENT)
              .setType("application/octet-stream").putExtra(Intent.EXTRA_TITLE, "ami-sync.ami")
            else Intent(Intent.ACTION_OPEN_DOCUMENT).setType("*/*")
          picker.launch(
            intent.addCategory(Intent.CATEGORY_OPENABLE).addFlags(
              Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
                Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
            )
          )
        }
        return
      }
      thread {
        reply(id, try {
          val ok = JSONObject().put("ok", true)
          when (op) {
            "read" -> ok.put(
              "text",
              contentResolver.openInputStream(content(uri))?.use { it.readBytes().toString(Charsets.UTF_8) } ?: ""
            )
            "write" -> {
              // "wt" truncates: a shorter file must not keep the tail of the old one.
              (contentResolver.openOutputStream(content(uri), "wt") ?: throw IOException("cannot open the file"))
                .use { it.write(text.toByteArray(Charsets.UTF_8)) }
              ok
            }
            else -> throw IllegalArgumentException("unknown operation $op")
          }
        } catch (e: Throwable) {
          failure(e)
        })
      }
    }
  }
}
