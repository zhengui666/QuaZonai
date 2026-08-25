import type { Locale } from './messages';

type RuntimeLabels = Record<Locale, string>;
const l = (en: string, zhCN: string, zhTW: string, ja: string, ko: string, es: string, ar: string): RuntimeLabels => ({ en, 'zh-CN': zhCN, 'zh-TW': zhTW, ja, ko, es, ar });

const labels: Record<string, RuntimeLabels> = {
  'Alpha Discovery': l('Alpha Discovery', 'Alpha 发现', 'Alpha 發現', 'Alpha探索', '알파 탐색', 'Descubrimiento Alpha', 'اكتشاف ألفا'),
  'Alpha Researcher': l('Alpha Researcher', 'Alpha 研究员', 'Alpha 研究員', 'Alpha研究者', '알파 연구원', 'Investigador Alpha', 'باحث ألفا'),
  'Program Created': l('Program Created', '项目已创建', '專案已建立', 'プログラム作成済み', '프로그램 생성됨', 'Programa creado', 'تم إنشاء البرنامج'),
  'Idea Contributed': l('Idea Contributed', '构想已贡献', '構想已貢獻', 'アイデア追加済み', '아이디어 기여됨', 'Idea aportada', 'تمت إضافة الفكرة'),
  'Program Paused': l('Program Paused', '项目已暂停', '專案已暫停', 'プログラム一時停止', '프로그램 일시정지됨', 'Programa pausado', 'تم إيقاف البرنامج مؤقتًا'),
  'Program Active': l('Program Active', '项目已激活', '專案已啟用', 'プログラム稼働中', '프로그램 활성', 'Programa activo', 'البرنامج نشط'),
  'Program Archived': l('Program Archived', '项目已归档', '專案已封存', 'プログラムアーカイブ済み', '프로그램 보관됨', 'Programa archivado', 'تمت أرشفة البرنامج'),
  'Mission Ready': l('Mission Ready', '任务就绪', '任務就緒', 'ミッション準備完了', '미션 준비됨', 'Misión lista', 'المهمة جاهزة'),
  'Mission Started': l('Mission Started', '任务已启动', '任務已啟動', 'ミッション開始', '미션 시작됨', 'Misión iniciada', 'بدأت المهمة'),
  'Mission Succeeded': l('Mission Succeeded', '任务成功', '任務成功', 'ミッション成功', '미션 성공', 'Misión completada', 'نجحت المهمة'),
  'Mission Failed': l('Mission Failed', '任务失败', '任務失敗', 'ミッション失敗', '미션 실패', 'Misión fallida', 'فشلت المهمة'),

  'Mandate Enabled': l('Mandate Enabled', '投资授权已启用', '投資授權已啟用', 'マンデート有効化', '만데이트 활성화됨', 'Mandato habilitado', 'تم تفعيل التفويض'),
  'Mandate Disabled': l('Mandate Disabled', '投资授权已禁用', '投資授權已停用', 'マンデート無効化', '만데이트 비활성화됨', 'Mandato deshabilitado', 'تم تعطيل التفويض'),
  'Approval Expired': l('Approval Expired', '审批已过期', '審批已過期', '承認期限切れ', '승인 만료됨', 'Aprobación vencida', 'انتهت صلاحية الموافقة'),
  'Approval Approved': l('Approval Approved', '审批已批准', '審批已核准', '承認済み', '승인됨', 'Aprobación aprobada', 'تمت الموافقة'),
  'Approval Rejected': l('Approval Rejected', '审批已拒绝', '審批已拒絕', '承認却下', '승인 거부됨', 'Aprobación rechazada', 'رُفضت الموافقة'),
  'Handoff Available': l('Handoff Available', '交付可领取', '交付可領取', 'ハンドオフ利用可能', '핸드오프 사용 가능', 'Entrega disponible', 'التسليم متاح'),
  'Handoff Revoked': l('Handoff Revoked', '交付已撤销', '交付已撤銷', 'ハンドオフ取消済み', '핸드오프 취소됨', 'Entrega revocada', 'تم إلغاء التسليم'),
  'Handoff Claimed': l('Handoff Claimed', '交付已领取', '交付已領取', 'ハンドオフ取得済み', '핸드오프 수령됨', 'Entrega reclamada', 'تم استلام التسليم'),
  'Handoff Accepted': l('Handoff Accepted', '交付已接受', '交付已接受', 'ハンドオフ受領済み', '핸드오프 수락됨', 'Entrega aceptada', 'تم قبول التسليم'),
  'Handoff Downstream Rejected': l('Handoff Downstream Rejected', '下游已拒绝交付', '下游已拒絕交付', '下流がハンドオフを拒否', '다운스트림이 핸드오프 거부', 'Entrega rechazada por el sistema downstream', 'رفض النظام اللاحق التسليم'),
  'Forward Evidence Recorded': l('Forward Evidence Recorded', '前向证据已记录', '前向證據已記錄', 'フォワード証拠記録済み', '포워드 증거 기록됨', 'Evidencia futura registrada', 'تم تسجيل الدليل المستقبلي'),
  'Handoff Feedback Status': l('Handoff Feedback Status', '交付反馈状态', '交付回饋狀態', 'ハンドオフフィードバック状態', '핸드오프 피드백 상태', 'Estado de comentarios de entrega', 'حالة ملاحظات التسليم'),
  'Data Source Registered': l('Data Source Registered', '数据源已注册', '資料來源已註冊', 'データソース登録済み', '데이터 소스 등록됨', 'Fuente de datos registrada', 'تم تسجيل مصدر البيانات'),
  'Downstream Registered': l('Downstream Registered', '下游系统已注册', '下游系統已註冊', '下流システム登録済み', '다운스트림 시스템 등록됨', 'Sistema downstream registrado', 'تم تسجيل النظام اللاحق'),
  'Downstream Service Token Rotated': l('Downstream Service Token Rotated', '下游服务令牌已轮换', '下游服務權杖已輪替', '下流サービストークン更新済み', '다운스트림 서비스 토큰 교체됨', 'Token de servicio downstream rotado', 'تم تدوير رمز خدمة النظام اللاحق'),

  'Job Leased': l('Job Leased', '作业已租用', '工作已租用', 'ジョブリース取得', '작업 임대됨', 'Trabajo arrendado', 'تم حجز المهمة'),
  'Job Failed': l('Job Failed', '作业失败', '工作失敗', 'ジョブ失敗', '작업 실패', 'Trabajo fallido', 'فشلت المهمة التشغيلية'),
  'Job Succeeded': l('Job Succeeded', '作业成功', '工作成功', 'ジョブ成功', '작업 성공', 'Trabajo completado', 'نجحت المهمة التشغيلية'),
  'Plugin Release Received': l('Plugin Release Received', '插件版本已接收', '外掛版本已接收', 'プラグインリリース受信済み', '플러그인 릴리스 수신됨', 'Versión de plugin recibida', 'تم استلام إصدار الإضافة'),
  'Plugin Release Activated': l('Plugin Release Activated', '插件版本已激活', '外掛版本已啟用', 'プラグインリリース有効化', '플러그인 릴리스 활성화됨', 'Versión de plugin activada', 'تم تفعيل إصدار الإضافة'),
  'Plugin Release Draining': l('Plugin Release Draining', '插件版本正在排空', '外掛版本正在排空', 'プラグインリリース排出中', '플러그인 릴리스 드레이닝 중', 'Versión de plugin en drenaje', 'إصدار الإضافة قيد الإخلاء'),
  'Plugin Release Remove Requested': l('Plugin Release Remove Requested', '已请求移除插件版本', '已要求移除外掛版本', 'プラグインリリース削除要求済み', '플러그인 릴리스 제거 요청됨', 'Eliminación de versión de plugin solicitada', 'تم طلب إزالة إصدار الإضافة'),
  'Plugin Release Failed': l('Plugin Release Failed', '插件版本失败', '外掛版本失敗', 'プラグインリリース失敗', '플러그인 릴리스 실패', 'Versión de plugin fallida', 'فشل إصدار الإضافة'),
  'Plugin Release Staged': l('Plugin Release Staged', '插件版本已暂存', '外掛版本已暫存', 'プラグインリリース検証済み', '플러그인 릴리스 스테이징됨', 'Versión de plugin preparada', 'تم تجهيز إصدار الإضافة'),
  'Plugin Bundle Ready': l('Plugin Bundle Ready', '插件运行包就绪', '外掛執行套件就緒', 'プラグインバンドル準備完了', '플러그인 번들 준비됨', 'Paquete de plugins listo', 'حزمة الإضافات جاهزة'),
  'Plugin Release Removed': l('Plugin Release Removed', '插件版本已移除', '外掛版本已移除', 'プラグインリリース削除済み', '플러그인 릴리스 제거됨', 'Versión de plugin eliminada', 'تمت إزالة إصدار الإضافة'),
  'Credential Set Created': l('Credential Set Created', '凭据集已创建', '憑證集已建立', '認証情報セット作成済み', '자격 증명 세트 생성됨', 'Conjunto de credenciales creado', 'تم إنشاء مجموعة بيانات الاعتماد'),
  'Credential Set Replaced': l('Credential Set Replaced', '凭据集已替换', '憑證集已替換', '認証情報セット置換済み', '자격 증명 세트 교체됨', 'Conjunto de credenciales reemplazado', 'تم استبدال مجموعة بيانات الاعتماد'),
  'Runtime Configuration Updated': l('Runtime Configuration Updated', '运行时配置已更新', '執行階段設定已更新', 'ランタイム設定更新済み', '런타임 구성 업데이트됨', 'Configuración de runtime actualizada', 'تم تحديث إعدادات وقت التشغيل'),
};

export const runtimeLabelSources = Object.freeze(Object.keys(labels));

export function translateRuntimeLabel(locale: Locale, source: string): string | undefined {
  return labels[source]?.[locale];
}
