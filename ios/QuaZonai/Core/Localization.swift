import SwiftUI

enum AppLanguage: String, CaseIterable, Codable, Identifiable {
    case english = "en"
    case simplifiedChinese = "zh-Hans"
    case traditionalChinese = "zh-Hant"
    case japanese = "ja"
    case korean = "ko"
    case spanish = "es"
    case arabic = "ar"

    var id: String { rawValue }
    var isRTL: Bool { self == .arabic }
    var displayName: String {
        switch self {
        case .english: "English"
        case .simplifiedChinese: "简体中文"
        case .traditionalChinese: "繁體中文"
        case .japanese: "日本語"
        case .korean: "한국어"
        case .spanish: "Español"
        case .arabic: "العربية"
        }
    }
}

enum AppAppearance: String, CaseIterable, Codable, Identifiable {
    case system, light, dark
    var id: String { rawValue }
    var colorScheme: ColorScheme? {
        switch self { case .system: nil; case .light: .light; case .dark: .dark }
    }
}

enum L10nKey: String, CaseIterable {
    case home, research, approvals, portfolio, more, idea, alpha, handoff, administration
    case researchObservatory, portfolioLab, approvalInbox
    case language, appearance, accountSecurity, server, connect, connecting, totp, trustDevice
    case signIn, unlock, logout, retry, loading, empty, error, search, sort, refresh
    case preview, startResearch, approve, reject, revoke, save, register, cancel, directWarning
    case secureServerHint, incompatible, recentEvents, systemHealth, actionCenter, settings
    case nativeAuthentication, nativeAuthenticationDescription, trustedDeviceDescription, useTOTP
}

enum L10n {
    private static let table: [L10nKey: [String]] = [
        .home: ["Home","首页","首頁","ホーム","홈","Inicio","الرئيسية"],
        .research: ["Research","研究","研究","リサーチ","리서치","Investigación","البحث"],
        .approvals: ["Approvals","审批","審批","承認","승인","Aprobaciones","الموافقات"],
        .portfolio: ["Portfolio","组合","組合","ポートフォリオ","포트폴리오","Cartera","المحفظة"],
        .more: ["More","更多","更多","その他","더보기","Más","المزيد"],
        .idea: ["Idea Composer","Idea 编辑器","Idea 編輯器","アイデア作成","아이디어 작성","Compositor de ideas","محرر الفكرة"],
        .alpha: ["Alpha Library","Alpha 库","Alpha 庫","アルファライブラリ","알파 라이브러리","Biblioteca Alpha","مكتبة ألفا"],
        .handoff: ["Handoff & Feedback","交付与反馈","交付與回饋","引き渡しとフィードバック","핸드오프 및 피드백","Entrega y feedback","التسليم والملاحظات"],
        .administration: ["Administration","管理","管理","管理","관리","Administración","الإدارة"],
        .researchObservatory: ["Research Observatory","研究观测台","研究觀測台","リサーチ・オブザーバトリー","리서치 관찰소","Observatorio de investigación","مرصد البحث"],
        .portfolioLab: ["Portfolio Lab","组合实验室","組合實驗室","ポートフォリオ・ラボ","포트폴리오 랩","Laboratorio de carteras","مختبر المحفظة"],
        .approvalInbox: ["Approval Inbox","审批收件箱","審批收件匣","承認受信箱","승인함","Bandeja de aprobaciones","صندوق الموافقات"],
        .language: ["Language","语言","語言","言語","언어","Idioma","اللغة"],
        .appearance: ["Appearance","外观","外觀","外観","화면 모드","Apariencia","المظهر"],
        .accountSecurity: ["Account / Device Security","账户 / 设备安全","帳戶 / 裝置安全","アカウント / 端末セキュリティ","계정 / 기기 보안","Cuenta / Seguridad del dispositivo","أمان الحساب / الجهاز"],
        .server: ["Server URL","服务器地址","伺服器位址","サーバー URL","서버 URL","URL del servidor","عنوان الخادم"],
        .connect: ["Connect","连接","連線","接続","연결","Conectar","اتصال"],
        .connecting: ["Connecting…","正在连接…","正在連線…","接続中…","연결 중…","Conectando…","جارٍ الاتصال…"],
        .totp: ["6-digit TOTP","6 位 TOTP","6 位 TOTP","6桁 TOTP","6자리 TOTP","TOTP de 6 dígitos","رمز TOTP من 6 أرقام"],
        .trustDevice: ["Trust this device","信任此设备","信任此裝置","この端末を信頼","이 기기 신뢰","Confiar en este dispositivo","الوثوق بهذا الجهاز"],
        .signIn: ["Sign in","登录","登入","サインイン","로그인","Iniciar sesión","تسجيل الدخول"],
        .unlock: ["Unlock trusted device","解锁受信任设备","解鎖受信任裝置","信頼済み端末を解除","신뢰 기기 잠금 해제","Desbloquear dispositivo","فتح الجهاز الموثوق"],
        .logout: ["Log out","退出登录","登出","ログアウト","로그아웃","Cerrar sesión","تسجيل الخروج"],
        .retry: ["Retry","重试","重試","再試行","다시 시도","Reintentar","إعادة المحاولة"],
        .loading: ["Loading…","加载中…","載入中…","読み込み中…","불러오는 중…","Cargando…","جارٍ التحميل…"],
        .empty: ["No data","暂无数据","暫無資料","データなし","데이터 없음","Sin datos","لا توجد بيانات"],
        .error: ["Error","错误","錯誤","エラー","오류","Error","خطأ"],
        .search: ["Search","搜索","搜尋","検索","검색","Buscar","بحث"],
        .sort: ["Sort","排序","排序","並べ替え","정렬","Ordenar","ترتيب"],
        .refresh: ["Refresh","刷新","重新整理","更新","새로고침","Actualizar","تحديث"],
        .preview: ["Preview research charter","预览研究章程","預覽研究章程","研究チャーターをプレビュー","연구 헌장 미리보기","Vista previa del charter","معاينة ميثاق البحث"],
        .startResearch: ["Start Research","开始研究","開始研究","研究開始","연구 시작","Iniciar investigación","بدء البحث"],
        .approve: ["Approve","批准","批准","承認","승인","Aprobar","موافقة"],
        .reject: ["Reject","拒绝","拒絕","却下","거절","Rechazar","رفض"],
        .revoke: ["Revoke","撤销","撤銷","取り消す","취소","Revocar","إلغاء"],
        .save: ["Save","保存","儲存","保存","저장","Guardar","حفظ"],
        .register: ["Register","注册","註冊","登録","등록","Registrar","تسجيل"],
        .cancel: ["Cancel","取消","取消","キャンセル","취소","Cancelar","إلغاء"],
        .directWarning: ["Authentication is disabled on this server. Anyone who can reach it has full Operator access.","此服务器已关闭认证；任何可访问它的人都拥有完整 Operator 权限。","此伺服器已關閉認證；任何可存取它的人都擁有完整 Operator 權限。","このサーバーは認証が無効です。到達できる全員が Operator 権限を持ちます。","이 서버는 인증이 꺼져 있어 접근 가능한 누구나 전체 Operator 권한을 가집니다.","La autenticación está desactivada; cualquiera con acceso de red tiene permisos de Operator.","المصادقة معطلة؛ أي شخص يصل إلى الخادم يملك صلاحيات المشغل كاملة."],
        .secureServerHint: ["Production servers must use HTTPS. HTTP is limited to localhost development.","生产服务器必须使用 HTTPS；HTTP 仅限 localhost 开发。","正式伺服器必須使用 HTTPS；HTTP 僅限 localhost 開發。","本番サーバーは HTTPS 必須です。HTTP は localhost 開発のみです。","운영 서버는 HTTPS가 필요하며 HTTP는 localhost 개발에서만 허용됩니다.","Los servidores de producción deben usar HTTPS; HTTP solo se permite en localhost.","يجب أن تستخدم خوادم الإنتاج HTTPS؛ ويقتصر HTTP على localhost للتطوير."],
        .incompatible: ["Update required","需要更新","需要更新","更新が必要","업데이트 필요","Actualización requerida","يلزم التحديث"],
        .recentEvents: ["Recent material events","近期重要事件","近期重要事件","最近の重要イベント","최근 주요 이벤트","Eventos materiales recientes","الأحداث المهمة الأخيرة"],
        .systemHealth: ["System Health","系统健康","系統健康","システム状態","시스템 상태","Salud del sistema","حالة النظام"],
        .actionCenter: ["Action Center","行动中心","行動中心","アクションセンター","액션 센터","Centro de acciones","مركز الإجراءات"],
        .settings: ["Settings","设置","設定","設定","설정","Ajustes","الإعدادات"],
        .nativeAuthentication: ["Native Operator Authentication","原生 Operator 认证","原生 Operator 認證","ネイティブ Operator 認証","네이티브 Operator 인증","Autenticación nativa del operador","مصادقة المشغل الأصلية"],
        .nativeAuthenticationDescription: ["This app authenticates with the current TOTP only. It never asks for or sends a username or password.","此应用仅使用当前 TOTP 认证，不会请求或发送用户名和密码。","此 App 僅使用目前 TOTP 認證，不會要求或傳送使用者名稱與密碼。","このアプリは現在の TOTP のみで認証し、ユーザー名やパスワードを要求・送信しません。","이 앱은 현재 TOTP만 사용하며 사용자 이름이나 비밀번호를 요청하거나 보내지 않습니다.","Esta app solo se autentica con el TOTP actual; nunca solicita ni envía usuario o contraseña.","يصادق هذا التطبيق باستخدام TOTP الحالي فقط؛ ولا يطلب اسم مستخدم أو كلمة مرور أو يرسلهما."],
        .trustedDeviceDescription: ["A trusted-device refresh credential is protected by this device's Keychain and biometrics.","受信任设备刷新凭据由此设备的钥匙串和生物识别保护。","受信任裝置更新憑證由此裝置的鑰匙圈與生物辨識保護。","信頼済み端末の更新資格情報は、この端末のキーチェーンと生体認証で保護されます。","신뢰 기기 갱신 자격 증명은 이 기기의 키체인과 생체 인증으로 보호됩니다.","La credencial de renovación del dispositivo confiable está protegida por el llavero y la biometría de este dispositivo.","تحمي سلسلة مفاتيح هذا الجهاز والقياسات الحيوية بيانات اعتماد التحديث للجهاز الموثوق."],
        .useTOTP: ["Use TOTP instead","改用 TOTP","改用 TOTP","TOTP を使う","TOTP 사용","Usar TOTP","استخدام TOTP بدلاً من ذلك"],
    ]

    static func text(_ key: L10nKey, _ language: AppLanguage) -> String {
        guard let values = table[key],
              let index = AppLanguage.allCases.firstIndex(of: language),
              values.indices.contains(index) else { return key.rawValue }
        return values[index]
    }

    static func validatesAllLanguages() -> Bool {
        L10nKey.allCases.allSatisfy { key in
            guard let values = table[key] else { return false }
            return values.count == AppLanguage.allCases.count && values.allSatisfy { !$0.isEmpty }
        }
    }
}
