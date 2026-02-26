package kas.kluster.kasia

import android.graphics.Color
import android.os.Bundle
import android.view.ViewGroup
import androidx.core.view.*


class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)

    // Edge-to-edge
    WindowCompat.setDecorFitsSystemWindows(window, false)
    window.statusBarColor = Color.TRANSPARENT
    window.navigationBarColor = Color.TRANSPARENT
    window.setSoftInputMode(
      android.view.WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE
    )

    // IMPORTANT: apply insets to the content container, not the WebView
    val content = findViewById<ViewGroup>(android.R.id.content)
    content.clipToPadding = false // let content draw under bars, we add padding

    ViewCompat.setOnApplyWindowInsetsListener(content, null)
    ViewCompat.setOnApplyWindowInsetsListener(content) { v, insets ->
      val bars = insets.getInsets(
        WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
      )
      val ime  = insets.getInsets(WindowInsetsCompat.Type.ime())

      // Push everything down/right/left + keep the bigger of nav bar or IME at bottom
      v.setPadding(bars.left, bars.top, bars.right, maxOf(bars.bottom, ime.bottom))

      android.util.Log.d("Insets",
        "applied -> top=${bars.top} left=${bars.left} right=${bars.right} " +
          "bottomBars=${bars.bottom} bottomIme=${ime.bottom}"
      )

      v.requestLayout()              // ensure a relayout after padding change
      insets                         // do NOT consume; let WebView still see them
    }

    ViewCompat.requestApplyInsets(content)
  }
}