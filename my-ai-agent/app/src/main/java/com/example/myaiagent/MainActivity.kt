package com.example.myaiagent

import android.content.Intent
import android.os.Bundle
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.example.myaiagent.command.Command
import com.example.myaiagent.execution.AgentAccessibilityService
import com.example.myaiagent.ui.AgentViewModel

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            MaterialTheme(colorScheme = lightColorScheme()) {
                CompositionLocalProvider(LocalLayoutDirection provides LayoutDirection.Rtl) {
                    AgentScreen(openSettings = { startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS)) })
                }
            }
        }
    }
}

@Composable
private fun AgentScreen(openSettings: () -> Unit, model: AgentViewModel = viewModel()) {
    val connected by AgentAccessibilityService.connected.collectAsStateWithLifecycle()
    val pending by model.pending.collectAsStateWithLifecycle()
    val running by model.running.collectAsStateWithLifecycle()
    val log by model.log.collectAsStateWithLifecycle()
    var input by remember { mutableStateOf("") }
    Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Text("مساعدي الذكي", style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold)
            Text("أوامر واضحة، تنفيذ تحت تحكمك", style = MaterialTheme.typography.bodyLarge)
            Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.secondaryContainer)) {
                Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("إمكانية الوصول", style = MaterialTheme.typography.titleMedium)
                    Text(if (connected) "الخدمة متصلة" else "الخدمة غير متصلة")
                    Text("فعّل الخدمة بنفسك لتنفيذ إجراءات الشاشة. يمكنك إيقافها من إعدادات النظام في أي وقت.")
                    OutlinedButton(onClick = openSettings) { Text("فتح إعدادات إمكانية الوصول") }
                }
            }
            OutlinedTextField(value = input, onValueChange = { input = it }, modifier = Modifier.fillMaxWidth(), label = { Text("اكتب أمرك بالعربية") }, minLines = 2)
            Text("أمثلة: افتح الخرائط، رجوع، الرئيسية، تمرير للأسفل، تمرير للأعلى، اضغط إرسال، اكتب مرحبا", style = MaterialTheme.typography.bodySmall)
            Text("إجراءات الشاشة تبدأ بعد ٥ ثوانٍ؛ انتقل خلالها إلى التطبيق المطلوب. لإدخال النص، حدد حقل الكتابة أولاً.", style = MaterialTheme.typography.bodyMedium)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(onClick = { model.submit(input) }, enabled = !running && pending == null) { Text("تنفيذ الأمر") }
                Button(onClick = model::stop, colors = ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error)) { Text("إيقاف") }
            }
            HorizontalDivider()
            Text("سجل التنفيذ", style = MaterialTheme.typography.titleLarge)
            if (log.isEmpty()) Text("لا توجد إجراءات بعد.")
            log.forEach { Text("• $it", style = MaterialTheme.typography.bodyMedium) }
        }
    }
    pending?.let { command ->
        val detail = when (command) {
            is Command.Tap -> "النقر على العنصر المطابق: ${command.label}"
            is Command.EnterText -> "استبدال محتوى الحقل المحدد بالنص: ${command.text}"
            else -> "تنفيذ الإجراء"
        }
        AlertDialog(onDismissRequest = model::dismiss, title = { Text("تأكيد الإجراء") }, text = { Text("$detail\nقد يؤدي هذا الإجراء إلى تغيير بيانات أو إرسالها. تأكد من الشاشة المستهدفة.") }, confirmButton = { TextButton(onClick = model::confirm) { Text("تأكيد") } }, dismissButton = { TextButton(onClick = model::dismiss) { Text("إلغاء") } })
    }
}
