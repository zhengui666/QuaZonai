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
};

export function translateRuntimeLabel(locale: Locale, source: string): string | undefined {
  return labels[source]?.[locale];
}
