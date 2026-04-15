export type Language = "en" | "zh-CN" | "fr-FR"

export function normalizeLanguage(language: unknown): Language {
    if (language === "zh" || language === "zh-CN" || language === "zh_CN") {
        return "zh-CN"
    }
    else if (language === "fr" || language === "fr-FR") {
        return "fr-FR"
    }
    return "en"
}

function getStoredSettings(): Record<string, unknown> | null {
    try {
        const raw = localStorage.getItem("mlSettings")
        return raw ? JSON.parse(raw) : null
    } catch {
        return null
    }
}

export function getCurrentLanguage(): Language {
    return normalizeLanguage(getStoredSettings()?.language)
}

export function hasStoredLanguage(): boolean {
    return getStoredSettings()?.language != null
}

export function adoptRoleDefaultLanguage(roleDefaultSettings: { language?: unknown } | null | undefined): boolean {
    if (hasStoredLanguage()) {
        return false
    }

    const roleLanguage = normalizeLanguage(roleDefaultSettings?.language)
    if (roleLanguage === getCurrentLanguage()) {
        return false
    }

    try {
        const settings = getStoredSettings() ?? {}
        settings.language = roleLanguage
        localStorage.setItem("mlSettings", JSON.stringify(settings))
        return true
    } catch {
        localStorage.setItem("mlSettings", JSON.stringify({ language: roleLanguage }))
        return true
    }
}

export function getLanguageOptions(): Array<{ value: Language, name: string }> {
    return [
        { value: "en", name: "English" },
        { value: "zh-CN", name: "中文" },
        { value: "fr-FR", name: "Français" }
    ]
}

export function getTranslations(language: Language) {
    if (language === "zh-CN") {
        return {
            index: {
                appTitle: "Moonlight 网页版",
                back: "返回",
                reload: "刷新",
                addHostUnreachable: (address: string) => `主机 "${address}" 无法访问`,
                saveSettingsFailed: "无法保存设置",
                rootNotFound: "找不到根元素",
            },
            stream: {
                missingHostOrApp: "缺少主机 ID 或应用 ID",
                fullscreenUnsupported: "你的浏览器不支持全屏。",
                fullscreenEscapeHint: "退出全屏需要按住 ESC 几秒。",
                pointerLockUnsupported: "浏览器不支持鼠标锁定",
                connecting: "正在连接",
                showLogs: "显示日志",
                hideLogs: "隐藏日志",
                close: "关闭",
                autoFullscreenPrompt: "是否进入全屏？",
                connectionComplete: "连接完成",
                serverMessage: (message: string) => `服务器：${message}`,
                sendKeycode: "发送按键码",
                lockMouse: "锁定鼠标",
                keyboard: "键盘",
                fullscreen: "全屏",
                stats: "统计",
                exit: "退出",
                mouseMode: "鼠标模式",
                touchMode: "触摸模式",
                relative: "相对模式",
                follow: "跟随模式",
                pointAndDrag: "点击拖动",
                touch: "触摸",
                localCursor: "本地光标",
                selectKeycode: "选择按键码",
                rootNotFound: "找不到根元素",
            },
            settings: {
                sidebar: "侧边栏",
                sidebarEdge: "侧边栏位置",
                left: "左",
                right: "右",
                up: "上",
                down: "下",
                video: "视频",
                bitrate: "码率 (Kbps)",
                fps: "帧率",
                videoSize: "视频分辨率",
                native: "原生",
                custom: "自定义",
                videoWidth: "视频宽度",
                videoHeight: "视频高度",
                videoFrameQueueSize: "视频帧队列大小",
                videoCodec: "视频编码",
                autoExperimental: "自动（实验性）",
                av1Experimental: "AV1（实验性）",
                forceVideoElementRenderer: "强制使用 Video Element 渲染器（仅 WebRTC）",
                useCanvasRenderer: "使用 Canvas 渲染器",
                canvasVsync: "Canvas 垂直同步（减少撕裂）",
                enableHdr: "启用 HDR",
                audio: "音频",
                playAudioLocal: "本地播放音频",
                audioSampleQueueSize: "音频采样队列大小",
                mouse: "鼠标",
                scrollMode: "滚动模式",
                startupMouseMode: "串流启动后鼠标模式",
                startupTouchMode: "串流启动后触摸模式",
                localCursorSensitivity: "本地光标灵敏度",
                highRes: "高精度",
                normal: "普通",
                controller: "手柄",
                controllerDisabled: "手柄（已禁用：需要安全上下文）",
                invertAB: "交换 A 和 B",
                invertXY: "交换 X 和 Y",
                overrideControllerInterval: "覆盖手柄状态发送间隔",
                other: "其他",
                language: "语言",
                dataTransport: "传输方式",
                auto: "自动",
                webSocket: "WebSocket",
                enterFullscreenOnStreamStart: "进入串流后弹窗全屏提示",
                saveRoleDefaults: "保存为当前角色默认设置",
                saveRoleDefaultsSuccess: "已将当前设置保存为当前角色默认",
                saveRoleDefaultsFailed: "保存角色默认设置失败",
                toggleFullscreenWithKeybind: "按 Ctrl + Shift + I 切换全屏和鼠标锁定",
                style: "样式",
                useCustomDropdown: "使用自定义下拉框实现",
            },
            addHost: {
                header: "主机",
                address: "地址",
                port: "端口",
            },
            admin: {
                rootNotFound: "找不到根元素",
                unauthorized: "你没有权限访问此页面！",
                users: "用户",
                roles: "角色",
                addUser: "添加用户",
                addRole: "添加角色",
                searchUser: "搜索用户",
                searchRole: "搜索角色",
                delete: "删除",
                apply: "应用",
                user: "用户",
                role: "角色",
                name: "名称",
                defaultPassword: "默认密码",
                moonlightClientId: "Moonlight 客户端 ID",
                pleaseSelectRole: "请选择角色！",
                roleExists: (name: string) => `已存在名为 "${name}" 的角色！`,
                userExists: (name: string) => `已存在名为 "${name}" 的用户！`,
                roleType: "类型",
                permissions: "权限",
                defaultSettings: "默认设置",
                userId: "用户 ID",
                userName: "用户名",
                password: "密码",
                newPassword: "新密码",
                roleId: "角色 ID",
                roleName: "角色名称",
                allowAddHosts: "允许添加主机",
                maximumBitrate: "最大码率 (Kbps)",
                allowH264: "允许 H264",
                allowH265: "允许 H265",
                allowAv1: "允许 AV1",
                allowHdr: "允许 HDR",
                allowWebrtc: "允许 WebRTC",
                allowWebSockets: "允许 WebSocket",
                roleDeleteBlocked: (users: string[]) => `要删除这个角色，需先删除或重新分配仍在使用该角色的用户。\n当前用户：\n${JSON.stringify(users)}`,
            },
            host: {
                showDetails: "显示详情",
                open: "打开",
                sendWakeUpPacket: "发送唤醒包",
                reload: "刷新",
                pair: "配对",
                makePrivate: "设为私有",
                makeGlobal: "设为全局",
                removeHost: "移除主机",
                failedToGetDetails: (id: number) => `无法获取主机 ${id} 的详情`,
                wakeUpSent: "已发送唤醒包。你的电脑可能需要一点时间才能启动。",
                alreadyPaired: "该主机已经配对！",
                pairPrompt: (name: string, pin: string) => `请在主机 ${name} 上输入以下 PIN 完成配对：\nPIN: ${pin}`,
                overwriteMismatch: (currentId: number, incomingId: number) => `尝试用主机 ${incomingId} 的数据覆盖主机 ${currentId}`,
                details: (host: any) =>
                    `Web Id: ${host.host_id}\n` +
                    `名称: ${host.name}\n` +
                    `配对状态: ${host.paired}\n` +
                    `状态: ${host.server_state}\n` +
                    `地址: ${host.address}\n` +
                    `HTTP 端口: ${host.http_port}\n` +
                    `HTTPS 端口: ${host.https_port}\n` +
                    `外部端口: ${host.external_port}\n` +
                    `版本: ${host.version}\n` +
                    `GFE 版本: ${host.gfe_version}\n` +
                    `唯一 ID: ${host.unique_id}\n` +
                    `MAC: ${host.mac}\n` +
                    `本地 IP: ${host.local_ip}\n` +
                    `当前游戏: ${host.current_game}\n` +
                    `HEVC 最大亮度像素: ${host.max_luma_pixels_hevc}\n` +
                    `服务器编解码支持: ${host.server_codec_mode_support}`,
            },
            game: {
                resumeSession: "恢复会话",
                stopCurrentSession: "停止当前会话",
                failedToCloseApp: "关闭应用失败！",
                showDetails: "显示详情",
                open: "打开",
                details: (app: any) =>
                    `标题: ${app.title}\n` +
                    `ID: ${app.app_id}\n` +
                    `支持 HDR: ${app.is_hdr_supported}\n`,
            },
            modal: {
                ok: "确定",
                cancel: "取消",
                login: "登录",
                username: "用户名",
                password: "密码",
                passwordAsFile: "从文件读取密码",
            },
            common: {
                openFile: "打开文件",
                notSelected: "（未选择）",
                missingContextMenu: "找不到上下文菜单元素",
                missingModalParent: "找不到弹窗父节点",
                missingModalOverlay: "找不到弹窗遮罩层",
                missingSidebar: "获取侧边栏失败",
            }
        }
    }
    if (language === "fr-FR") {
        return {
            index: {
                appTitle: "Moonlight Web",
                back: "Retour",
                reload: "Recharger",
                addHostUnreachable: (address: string) => `L'hôte "${address}" est injoignable`,
                saveSettingsFailed: "Echec de l'enregistrement des paramètres",
                rootNotFound: "Elément racine introuvalbe",
            },
            stream: {
                missingHostOrApp: "Aucun hôte ou identifiant d'application trouvé",
                fullscreenUnsupported: "Mode plein écran non supporté par votre navigateur !",
                fullscreenEscapeHint: "Pour quitter le mode plein écran, vous devrez maintenir la touche Echap appuyée quelques secondes.",
                pointerLockUnsupported: "Verrouillage du pointeur non supporté",
                connecting: "Connexion",
                showLogs: "Afficher les journaux",
                hideLogs: "Cacher les journaux",
                close: "Fermer",
                autoFullscreenPrompt: "Basculer en mode plein écran ?",
                connectionComplete: "Connexion établie",
                serverMessage: (message: string) => `Serveur: ${message}`,
                sendKeycode: "Envoiyer le code clé",
                lockMouse: "Verrouillage de la souris",
                keyboard: "Clavier",
                fullscreen: "Plein écran",
                stats: "Stats",
                exit: "Quitter",
                mouseMode: "Mode souris",
                touchMode: "Mode tactile",
                relative: "Relative",
                follow: "Suivre",
                pointAndDrag: "Pointer et glisser",
                touch: "Tactiel",
                localCursor: "Curseur local",
                selectKeycode: "Selection du code clé",
                rootNotFound: "Elément racine introuvable",
            },
            settings: {
                sidebar: "Barre latérale",
                sidebarEdge: "Bord de la barre latérale",
                left: "Gauche",
                right: "Droite",
                up: "Haut",
                down: "Bas",
                video: "Vidéo",
                bitrate: "Débit (Kbps)",
                fps: "Ips",
                videoSize: "Taille vidéo",
                native: "native",
                custom: "personnalisée",
                videoWidth: "Largeur vidéo",
                videoHeight: "Hauteur vidéo",
                videoFrameQueueSize: "Taille file d'attente vidéo",
                videoCodec: "Codec vidéo",
                autoExperimental: "Auto (expérimental)",
                av1Experimental: "AV1 (expérimental)",
                forceVideoElementRenderer: "Forcer le rendu Video Element (uniquement WebRTC)",
                useCanvasRenderer: "Utiliser le rendu Canvas",
                canvasVsync: "VSync Canvas (réduit le déchirement)",
                enableHdr: "Activer HDR",
                audio: "Audio",
                playAudioLocal: "Jouer l'auudio localement",
                audioSampleQueueSize: "Taille de la file d'attente audio",
                mouse: "Souris",
                scrollMode: "Mode de défilement",
                startupMouseMode: "Mode de la souris au démarrage",
                startupTouchMode: "Mode tactile au démarrage",
                localCursorSensitivity: "Sensibilité du curseur local",
                highRes: "Haute résolution",
                normal: "Normal",
                controller: "Contrôleur",
                controllerDisabled: "Contrôleur (Désactivé: Context sécurisé requis)",
                invertAB: "Inverser A et B",
                invertXY: "Inverser X et Y",
                overrideControllerInterval: "Outrepasser l'intervalle d'envoi de l'état du contrôleur",
                other: "Autre",
                language: "Langue",
                dataTransport: "Transport des données",
                auto: "Auto",
                webSocket: "Web Socket",
                enterFullscreenOnStreamStart: "Invite de mise en plein écran au démarrage de la diffusion",
                saveRoleDefaults: "Enregistrer par défaut pour le rôle",
                saveRoleDefaultsSuccess: "Enregistré par défaut pour le rôle",
                saveRoleDefaultsFailed: "Echec de l'enregistrement des paramètres par défaut pour le rôle",
                toggleFullscreenWithKeybind: "Basculer entre le mode plein écran et le verrouillage de la souris avec Ctrl + Shift + I",
                style: "Style",
                useCustomDropdown: "Utiliser l'implémentation personnalisée du déroulement",
            },
            addHost: {
                header: "Hôte",
                address: "Adresse",
                port: "Port",
            },
            admin: {
                rootNotFound: "Elément racine introuvable",
                unauthorized: "Vous n'êtes pas autorisé à visualiser cette page !",
                users: "Utilisateurs",
                roles: "Rôles",
                addUser: "Ajout d'un utilisateur",
                addRole: "Ajout d'un rôle",
                searchUser: "Recherche Utilisateur",
                searchRole: "Recherche Rôle",
                delete: "Supprimer",
                apply: "Appliquer",
                user: "Utilisateur",
                role: "Rôle",
                name: "Nom",
                defaultPassword: "Mot de passe par défaut",
                moonlightClientId: "Identifiant du client Moonlight",
                pleaseSelectRole: "Veuillez choisir un rôle !",
                roleExists: (name: string) => `Nom du rôle "${name}" déjà pris !`,
                userExists: (name: string) => `Nom de l'utilisateur "${name}" déjà pris !`,
                roleType: "Type",
                permissions: "Permissions",
                defaultSettings: "Paramètres par défaut",
                userId: "Identifiant de l'utilisateur",
                userName: "Nom de l'utilisateur",
                password: "Mot de passe",
                newPassword: "Nouveau mot de passe",
                roleId: "Identifiant du rôle",
                roleName: "Nom du rôle",
                allowAddHosts: "Autoriser l'ajout d'hôtes",
                maximumBitrate: "Débit maximum (Kbps)",
                allowH264: "Permettre H264",
                allowH265: "Permettre H265",
                allowAv1: "Permettre Av1",
                allowHdr: "Permettre HDR",
                allowWebrtc: "Permettre WebRTC",
                allowWebSockets: "Permettre Web Sockets",
                roleDeleteBlocked: (users: string[]) => `Pour supprimer ce rôle, tous les utilisateurs associés à ce rôle doivent être supprimés ou associés à un autre rôle.\nUtilisateurs actuellement associés à ce rôle:\n${JSON.stringify(users)}`,
            },
            host: {
                showDetails: "Afficher les détails",
                open: "Ouvrir",
                sendWakeUpPacket: "Envoyer le signal de réveil",
                reload: "Recharger",
                pair: "Appairer",
                makePrivate: "Rendre prvivé",
                makeGlobal: "Rendre global",
                removeHost: "Retirer l'hôte",
                failedToGetDetails: (id: number) => `échec de la récupération des détails de l'hôte ${id}`,
                wakeUpSent: "Signal de réveil envoyé. Le PC peut prendre un peu de temps pour démarrer.",
                alreadyPaired: "Hôte déjà appairé !",
                pairPrompt: (name: string, pin: string) => `Veuillez appairer votre hôte ${name} avec ce NIP :\nNIP: ${pin}`,
                overwriteMismatch: (currentId: number, incomingId: number) => `tentative d'écrasement de l'hôte ${currentId} avec les données de ${incomingId}`,
                details: (host: any) =>
                    `Identifiant Web : ${host.host_id}\n` +
                    `Nom : ${host.name}\n` +
                    `Status d'appairage : ${host.paired}\n` +
                    `Etat : ${host.server_state}\n` +
                    `Adresse:  ${host.address}\n` +
                    `Port HTTP : ${host.http_port}\n` +
                    `Port HTTPS : ${host.https_port}\n` +
                    `Port Externe : ${host.external_port}\n` +
                    `Version: ${host.version}\n` +
                    `Version GFE : ${host.gfe_version}\n` +
                    `Identifiant unique : ${host.unique_id}\n` +
                    `MAC: ${host.mac}\n` +
                    `IP locale : ${host.local_ip}\n` +
                    `Jeu actuel : ${host.current_game}\n` +
                    `Max des Pixels Luma HEVC : ${host.max_luma_pixels_hevc}\n` +
                    `Mode de support du codec serveur : ${host.server_codec_mode_support}`,
            },
            game: {
                resumeSession: "Reprise de la session",
                stopCurrentSession: "Arrêt de la session",
                failedToCloseApp: "Echec de la fermeture de l'app !",
                showDetails: "Afficher les détails",
                open: "Ouvrir",
                details: (app: any) =>
                    `Titre : ${app.title}\n` +
                    `Identifiant : ${app.app_id}\n` +
                    `Support du HDR : ${app.is_hdr_supported}\n`,
            },
            modal: {
                ok: "Ok",
                cancel: "Annuler",
                login: "Connecter",
                username: "Nom d'utilisateur",
                password: "Mot de passe",
                passwordAsFile: "Fichier de mot de passe",
            },
            common: {
                openFile: "Ouvrir un fichier",
                notSelected: "(Non sélectionné)",
                missingContextMenu: "élément de menu contextuel introuvable",
                missingModalParent: "parent modal introuvable",
                missingModalOverlay: "couverture modale introuvable",
                missingSidebar: "echec de l'obtention de la barre latérale",
            }
        }
    }

    return {
        index: {
            appTitle: "Moonlight Web",
            back: "Back",
            reload: "Reload",
            addHostUnreachable: (address: string) => `Host "${address}" is not reachable`,
            saveSettingsFailed: "Couldn't save settings",
            rootNotFound: "couldn't find root element",
        },
        stream: {
            missingHostOrApp: "No Host or no App Id found",
            fullscreenUnsupported: "Fullscreen is not supported by your browser!",
            fullscreenEscapeHint: "To exit Fullscreen you'll have to hold ESC for a few seconds.",
            pointerLockUnsupported: "Pointer Lock not supported",
            connecting: "Connecting",
            showLogs: "Show Logs",
            hideLogs: "Hide Logs",
            close: "Close",
            autoFullscreenPrompt: "Enter fullscreen now?",
            connectionComplete: "Connection Complete",
            serverMessage: (message: string) => `Server: ${message}`,
            sendKeycode: "Send Keycode",
            lockMouse: "Lock Mouse",
            keyboard: "Keyboard",
            fullscreen: "Fullscreen",
            stats: "Stats",
            exit: "Exit",
            mouseMode: "Mouse Mode",
            touchMode: "Touch Mode",
            relative: "Relative",
            follow: "Follow",
            pointAndDrag: "Point and Drag",
            touch: "Touch",
            localCursor: "Local Cursor",
            selectKeycode: "Select Keycode",
            rootNotFound: "couldn't find root element",
        },
        settings: {
            sidebar: "Sidebar",
            sidebarEdge: "Sidebar Edge",
            left: "Left",
            right: "Right",
            up: "Up",
            down: "Down",
            video: "Video",
            bitrate: "Bitrate (Kbps)",
            fps: "Fps",
            videoSize: "Video Size",
            native: "native",
            custom: "custom",
            videoWidth: "Video Width",
            videoHeight: "Video Height",
            videoFrameQueueSize: "Video Frame Queue Size",
            videoCodec: "Video Codec",
            autoExperimental: "Auto (Experimental)",
            av1Experimental: "AV1 (Experimental)",
            forceVideoElementRenderer: "Force Video Element Renderer (WebRTC only)",
            useCanvasRenderer: "Use Canvas Renderer",
            canvasVsync: "Canvas VSync (reduce tearing)",
            enableHdr: "Enable HDR",
            audio: "Audio",
            playAudioLocal: "Play Audio Local",
            audioSampleQueueSize: "Audio Sample Queue Size",
            mouse: "Mouse",
            scrollMode: "Scroll Mode",
            startupMouseMode: "Mouse Mode On Stream Start",
            startupTouchMode: "Touch Mode On Stream Start",
            localCursorSensitivity: "Local Cursor Sensitivity",
            highRes: "High Res",
            normal: "Normal",
            controller: "Controller",
            controllerDisabled: "Controller (Disabled: Secure Context Required)",
            invertAB: "Invert A and B",
            invertXY: "Invert X and Y",
            overrideControllerInterval: "Override Controller State Send Interval",
            other: "Other",
            language: "Language",
            dataTransport: "Data Transport",
            auto: "Auto",
            webSocket: "Web Socket",
            enterFullscreenOnStreamStart: "Prompt Fullscreen On Stream Start",
            saveRoleDefaults: "Save As Role Defaults",
            saveRoleDefaultsSuccess: "Saved current settings as role defaults",
            saveRoleDefaultsFailed: "Couldn't save role default settings",
            toggleFullscreenWithKeybind: "Toggle Fullscreen and Mouse Lock with Ctrl + Shift + I",
            style: "Style",
            useCustomDropdown: "Use Custom Dropdown Implementation",
        },
        addHost: {
            header: "Host",
            address: "Address",
            port: "Port",
        },
        admin: {
            rootNotFound: "couldn't find root element",
            unauthorized: "You are not authorized to view this page!",
            users: "Users",
            roles: "Roles",
            addUser: "Add User",
            addRole: "Add Role",
            searchUser: "Search User",
            searchRole: "Search Role",
            delete: "Delete",
            apply: "Apply",
            user: "User",
            role: "Role",
            name: "Name",
            defaultPassword: "Default Password",
            moonlightClientId: "Moonlight Client Id",
            pleaseSelectRole: "Please select a role!",
            roleExists: (name: string) => `A role with the name "${name}" already exists!`,
            userExists: (name: string) => `A user with the name "${name}" already exists!`,
            roleType: "Type",
            permissions: "Permissions",
            defaultSettings: "Default Settings",
            userId: "User Id",
            userName: "User Name",
            password: "Password",
            newPassword: "New Password",
            roleId: "Role Id",
            roleName: "Role Name",
            allowAddHosts: "Allow adding Hosts",
            maximumBitrate: "Maximum Bitrate (Kbps)",
            allowH264: "Allow H264",
            allowH265: "Allow H265",
            allowAv1: "Allow Av1",
            allowHdr: "Allow HDR",
            allowWebrtc: "Allow WebRTC",
            allowWebSockets: "Allow Web Sockets",
            roleDeleteBlocked: (users: string[]) => `To remove this role all users that are currently assigned this role either need to be deleted or assigned another role.\nCurrently these users still have the role:\n${JSON.stringify(users)}`,
        },
        host: {
            showDetails: "Show Details",
            open: "Open",
            sendWakeUpPacket: "Send Wake Up Packet",
            reload: "Reload",
            pair: "Pair",
            makePrivate: "Make Private",
            makeGlobal: "Make Global",
            removeHost: "Remove Host",
            failedToGetDetails: (id: number) => `failed to get details for host ${id}`,
            wakeUpSent: "Sent Wake Up packet. It might take a moment for your pc to start.",
            alreadyPaired: "This host is already paired!",
            pairPrompt: (name: string, pin: string) => `Please pair your host ${name} with this pin:\nPin: ${pin}`,
            overwriteMismatch: (currentId: number, incomingId: number) => `tried to overwrite host ${currentId} with data from ${incomingId}`,
            details: (host: any) =>
                `Web Id: ${host.host_id}\n` +
                `Name: ${host.name}\n` +
                `Pair Status: ${host.paired}\n` +
                `State: ${host.server_state}\n` +
                `Address: ${host.address}\n` +
                `Http Port: ${host.http_port}\n` +
                `Https Port: ${host.https_port}\n` +
                `External Port: ${host.external_port}\n` +
                `Version: ${host.version}\n` +
                `Gfe Version: ${host.gfe_version}\n` +
                `Unique ID: ${host.unique_id}\n` +
                `MAC: ${host.mac}\n` +
                `Local IP: ${host.local_ip}\n` +
                `Current Game: ${host.current_game}\n` +
                `Max Luma Pixels Hevc: ${host.max_luma_pixels_hevc}\n` +
                `Server Codec Mode Support: ${host.server_codec_mode_support}`,
        },
        game: {
            resumeSession: "Resume Session",
            stopCurrentSession: "Stop Current Session",
            failedToCloseApp: "Failed to close app!",
            showDetails: "Show Details",
            open: "Open",
            details: (app: any) =>
                `Title: ${app.title}\n` +
                `Id: ${app.app_id}\n` +
                `HDR Supported: ${app.is_hdr_supported}\n`,
        },
        modal: {
            ok: "Ok",
            cancel: "Cancel",
            login: "Login",
            username: "Username",
            password: "Password",
            passwordAsFile: "Password as File",
        },
        common: {
            openFile: "Open File",
            notSelected: "(Not Selected)",
            missingContextMenu: "cannot find the context menu element",
            missingModalParent: "cannot find modal parent",
            missingModalOverlay: "the modal overlay cannot be found",
            missingSidebar: "failed to get sidebar",
        }
    }
}
