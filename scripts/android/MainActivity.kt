package com.seo1323.amicalculator

import android.os.Bundle
import android.view.View
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

// Copied over the generated MainActivity by CI (and by hand for local builds, see PLAN.md).
// Android WebView reports env(safe-area-inset-*) for the display cutout only, so with edge-to-edge
// the page would sit under the status bar and the gesture bar. The insets are applied here as
// padding instead, and consumed so the page does not add them a second time.
class MainActivity : TauriActivity() {
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
}
