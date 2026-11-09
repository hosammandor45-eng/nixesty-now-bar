package com.example.myaiagent.execution

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.provider.Settings
import com.example.myaiagent.command.NameMatcher

data class AppChoice(val label: String, val packageName: String, val settings: Boolean = false)

object AppLauncher {
    fun resolve(context: Context, name: String): List<AppChoice> {
        val pm = context.packageManager
        if (NameMatcher.isSettings(name)) {
            val resolved = Intent(Settings.ACTION_SETTINGS).resolveActivity(pm) ?: return emptyList()
            return listOf(AppChoice("الإعدادات / Settings", resolved.packageName, true))
        }
        val names = NameMatcher.equivalentNames(name)
        val query = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER)
        return pm.queryIntentActivities(query, 0).mapNotNull {
            val label = it.loadLabel(pm).toString()
            if (NameMatcher.normalize(label) in names || it.activityInfo.packageName in NameMatcher.knownPackages(name)) AppChoice(label, it.activityInfo.packageName) else null
        }.distinctBy { it.packageName }.sortedWith(compareBy({ it.label }, { it.packageName }))
    }

    fun open(context: Context, choice: AppChoice): Boolean {
        val intent = if (choice.settings) Intent(Settings.ACTION_SETTINGS)
            else context.packageManager.getLaunchIntentForPackage(choice.packageName) ?: return false
        return try { context.startActivity(intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)); true }
        catch (_: SecurityException) { false }
        catch (_: ActivityNotFoundException) { false }
    }
}
