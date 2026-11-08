package com.example.myaiagent.execution

import android.content.Context
import android.content.Intent
import com.example.myaiagent.command.Command

object AppLauncher {
    fun open(context: Context, command: Command.OpenApp): String {
        val pm = context.packageManager
        val query = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER)
        val matches = pm.queryIntentActivities(query, 0).filter {
            it.loadLabel(pm).toString().equals(command.name, ignoreCase = true)
        }.distinctBy { it.activityInfo.packageName }
        if (matches.size != 1) return "لم يُعثر على تطبيق واحد بهذا الاسم. استخدم الاسم الظاهر في قائمة التطبيقات."
        val intent = pm.getLaunchIntentForPackage(matches.single().activityInfo.packageName)
            ?: return "لا يمكن فتح هذا التطبيق."
        return try { context.startActivity(intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)); "تم فتح التطبيق." }
        catch (_: SecurityException) { "لا يسمح النظام بفتح هذا التطبيق." }
        catch (_: android.content.ActivityNotFoundException) { "لم يعد التطبيق متاحاً." }
    }
}
