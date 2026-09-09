package io.github.zniece.yaclue

import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import java.io.File

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    extractBundledScripts()
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }

  private fun extractBundledScripts() {
    val bundledRoot = File(filesDir, "bundled")
    val versionRoot = File(bundledRoot, BuildConfig.VERSION_NAME)
    val marker = File(versionRoot, ".complete")
    if (marker.isFile) return

    bundledRoot.listFiles()?.forEach { if (it != versionRoot) it.deleteRecursively() }
    versionRoot.deleteRecursively()
    copyAssetTree("yacas", File(versionRoot, "yacas"))
    copyAssetTree("processing", File(versionRoot, "processing"))
    marker.parentFile?.mkdirs()
    marker.writeText(BuildConfig.VERSION_NAME)
  }

  private fun copyAssetTree(assetPath: String, destination: File) {
    val children = assets.list(assetPath).orEmpty()
    if (children.isEmpty()) {
      destination.parentFile?.mkdirs()
      assets.open(assetPath).use { input ->
        destination.outputStream().use { output -> input.copyTo(output) }
      }
      return
    }

    destination.mkdirs()
    children.forEach { child ->
      copyAssetTree("$assetPath/$child", File(destination, child))
    }
  }
}
