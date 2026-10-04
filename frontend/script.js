// ============================================================================
// LocalPersona (fork of LocalChat Studio) — Main Application Script
// This is the original frontend logic. It will be gradually enhanced with
// Tauri IPC calls for model discovery, server control, and persistent storage.
// ============================================================================

// --- MODE DEFINITIONS ---
const MODES = {
    'advanced-chat': { icon: '🎛️', label: 'Advanced Chat', systemPrefix: 'You are an advanced AI chat partner.' },
    'adventure': { icon: '🧙‍♂️', label: 'Adventure', systemPrefix: 'You are the narrator of an immersive adventure. Describe scenes vividly, react to the player\'s actions, and drive the story forward. Use 2nd person ("You walk into the tavern..."). Keep the adventure exciting and responsive to player choices.' },
    'story': { icon: '📖', label: 'Story', systemPrefix: 'You are a collaborative storyteller. Write vivid, engaging narrative prose. Build on the user\'s contributions and weave a compelling story together. Use descriptive language and develop characters naturally.' },
    'image-gen': { icon: '🖼️', label: 'Image Gen', systemPrefix: 'You are an AI that generates detailed visual image descriptions. When the user requests an image, describe it in vivid, precise detail — composition, lighting, colors, mood, style, and subject. Write as if describing a real photograph or artwork.' },
};

// --- STATE MANAGEMENT ---
const APP_STATE = {
    characters: [],
    activeCharacterId: null,
    viewMode: 'welcome',
    activeVoice: 'user',
    settings: {
        apiType: 'openai-compatible',
        apiEndpoint: 'http://localhost:8080/v1/chat/completions',
        model: 'local-model',
        temperature: 0.7,
        maxTokens: 2048,
        topP: 0.9,
        stream: false, // R2-A: streaming frozen for RC-stable (non-stream path only)
        ttsEndpoint: 'http://localhost:8081/v1/audio/speech',
        ttsModelPath: 'test-models/qwen3-tts-12hz-0.6b-customvoice-q8_0.gguf',
    },
    isConnected: false,
    isGenerating: false,
    streamingContent: '',
    abortController: null,
};

// Performance: incremental conversation rendering state
let renderedMessageIds = new Set();
let currentConversationId = null;
let messageLoadOffset = 0;
const MESSAGES_PAGE_SIZE = 50;
let isLoadingMoreMessages = false;

// Conversation previews cache for sidebar (last message, timestamps)
let conversationPreviews = [];

const MODE_ICONS = {
    'advanced-chat': '🎛️',
    'adventure': '🧙‍♂️',
    'story': '📖',
    'character-gen': '✨',
};

const DEFAULT_CHARACTERS = [
    {
        id: 'default-friendly',
        name: 'Friendly Chat',
        mode: 'advanced-chat',
        avatarColor: '#7c3aed',
        avatarUrl: '',
        personality: 'A warm, empathetic AI companion who loves casual conversation, sharing stories, and making people feel heard.',
        userNickname: '',
        userDescription: '',
        scenario: '',
        writingInstructions: '',
        systemPrompt: 'You are a warm, friendly, and empathetic AI companion. You love casual conversation, share interesting thoughts, ask thoughtful questions, and make people feel comfortable and heard. Keep responses natural and engaging, like chatting with a good friend. Avoid being overly formal or robotic.',
        greeting: 'Hey there! 👋 So happy to chat with you. How\'s your day going? I\'d love to hear about whatever\'s on your mind!',
        createdAt: Date.now(),
    },
    {
        id: 'default-writer',
        name: 'Creative Writer',
        mode: 'story',
        avatarColor: '#ec4899',
        avatarUrl: '',
        personality: 'A passionate wordsmith who helps with creative writing, storytelling, world-building, and brainstorming ideas.',
        userNickname: '',
        userDescription: '',
        scenario: '',
        writingInstructions: '',
        systemPrompt: 'You are a passionate creative writer and storyteller. You help users develop stories, create characters, build worlds, and brainstorm creative ideas. You provide constructive feedback and inspiring suggestions. Write in an engaging, vivid style and encourage the user\'s creativity.',
        greeting: 'Welcome, fellow storyteller! ✨ I\'m here to help you craft amazing tales, develop characters, or brainstorm whatever creative project is on your mind. What shall we create together?',
        createdAt: Date.now() - 1000,
    },
    {
        id: 'default-coder',
        name: 'Code Assistant',
        mode: 'advanced-chat',
        avatarColor: '#10b981',
        avatarUrl: '',
        personality: 'A knowledgeable programming assistant who helps debug code, explain concepts, and build software solutions.',
        userNickname: '',
        userDescription: '',
        scenario: '',
        writingInstructions: '',
        systemPrompt: 'You are an expert programming assistant. You help users write, debug, and understand code across multiple languages. Explain concepts clearly, provide working code examples, and follow best practices. Be concise but thorough. When providing code, use proper formatting and include brief explanations.',
        greeting: 'Ready to code! 💻 I can help with debugging, writing new features, explaining concepts, or reviewing your code. What programming challenge can I help you tackle?',
        createdAt: Date.now() - 2000,
    },
    {
        id: 'default-explorer',
        name: 'Knowledge Explorer',
        mode: 'advanced-chat',
        avatarColor: '#f59e0b',
        avatarUrl: '',
        personality: 'A curious intellectual who loves diving deep into topics, explaining complex ideas simply, and exploring new subjects.',
        userNickname: '',
        userDescription: '',
        scenario: '',
        writingInstructions: '',
        systemPrompt: 'You are a curious and knowledgeable explorer of ideas. You love diving deep into topics, explaining complex concepts in simple terms, and making learning enjoyable. Draw connections between different fields and always encourage curiosity. Be thorough but accessible.',
        greeting: 'Curiosity is the best compass! 🧭 I love exploring fascinating topics and breaking down complex ideas. What subject shall we dive into today? Science, philosophy, history, technology — you name it!',
        createdAt: Date.now() - 3000,
    },
    {
        id: 'default-roleplay',
        name: 'Roleplay Partner',
        mode: 'adventure',
        avatarColor: '#3b82f6',
        avatarUrl: '',
        personality: 'An immersive roleplaying partner who adapts to any scenario, character, or setting with creativity and enthusiasm.',
        userNickname: '',
        userDescription: '',
        scenario: '',
        writingInstructions: '',
        systemPrompt: 'You are an immersive and creative roleplaying partner. You adapt to any scenario, character, or setting the user suggests. Stay in character, describe actions and environments vividly, and respond naturally to keep the story engaging. Use asterisks for actions and maintain consistent character voice.',
        greeting: '*A friendly smile spreads across their face as they lean forward with interest.* Welcome! I\'m ready to jump into any world or scenario you\'d like. What kind of adventure shall we embark on? Fantasy, sci-fi, historical, or something entirely unique?',
        createdAt: Date.now() - 4000,
    },
];

// --- PERSISTENCE HELPERS (Phase 1 - Single Source of Truth: Disk) ---
//
// Characters and conversation histories are now persisted exclusively via the Rust backend.
// localStorage is restricted to settings and lightweight UI state only.

function saveToStorage(key, data) {
    try {
        localStorage.setItem(`localpersona_${key}`, JSON.stringify(data));
    } catch (e) {
        console.warn('Failed to save to localStorage:', e);
    }
}

function loadFromStorage(key, fallback = null) {
    try {
        const data = localStorage.getItem(`localpersona_${key}`);
        return data ? JSON.parse(data) : fallback;
    } catch (e) {
        console.warn('Failed to load from localStorage:', e);
        return fallback;
    }
}

function getChatHistoryKey(characterId) {
    return `chat_history_${characterId}`;
}

// === Disk-based character persistence (Phase 1) ===
// UI uses camelCase; Rust StoredCharacter serializes camelCase with snake_case aliases.
// Normalize both directions so legacy on-disk JSON still works.

function normalizeCharacter(raw) {
    if (!raw || typeof raw !== 'object') return raw;
    const avatarUrl = raw.avatarUrl ?? raw.avatar_path ?? raw.avatarPath ?? '';
    return {
        ...raw,
        id: raw.id,
        name: raw.name || 'Unnamed',
        mode: raw.mode || 'advanced-chat',
        avatarColor: raw.avatarColor ?? raw.avatar_color ?? '#7c3aed',
        avatarUrl: avatarUrl || '',
        personality: raw.personality || '',
        userNickname: raw.userNickname ?? raw.user_nickname ?? '',
        userDescription: raw.userDescription ?? raw.user_description ?? '',
        scenario: raw.scenario ?? '',
        writingInstructions: raw.writingInstructions ?? raw.writing_instructions ?? '',
        systemPrompt: raw.systemPrompt ?? raw.system_prompt ?? '',
        greeting: raw.greeting || `Hello! I'm ${raw.name || 'there'}.`,
        createdAt: raw.createdAt ?? raw.created_at ?? Date.now(),
        voiceMode: raw.voiceMode ?? raw.voice_mode ?? 'none',
        voicePreset: raw.voicePreset ?? raw.voice_preset ?? null,
        voiceSamplePath: raw.voiceSamplePath ?? raw.voice_sample_path ?? null,
        spawnLocation: raw.spawnLocation ?? raw.spawn_location ?? null,
        isPublicSpawn: raw.isPublicSpawn ?? raw.is_public_spawn ?? false,
        has_knowledge_base: raw.has_knowledge_base ?? raw.hasKnowledgeBase ?? false,
        version: raw.version ?? 1,
    };
}

/** Shape expected by Rust StoredCharacter (camelCase serde). */
function characterToDisk(char) {
    const n = normalizeCharacter(char);
    return {
        version: n.version ?? 1,
        id: n.id,
        name: n.name,
        mode: n.mode || 'advanced-chat',
        avatarColor: n.avatarColor || '#7c3aed',
        avatarUrl: n.avatarUrl || null,
        personality: n.personality || '',
        userNickname: n.userNickname || null,
        userDescription: n.userDescription || null,
        scenario: n.scenario || null,
        writingInstructions: n.writingInstructions || null,
        systemPrompt: n.systemPrompt || null,
        greeting: n.greeting || '',
        createdAt: typeof n.createdAt === 'number' ? n.createdAt : Date.now(),
        voiceMode: n.voiceMode || 'none',
        voicePreset: n.voicePreset || null,
        voiceSamplePath: n.voiceSamplePath || null,
        spawnLocation: n.spawnLocation || null,
        isPublicSpawn: !!n.isPublicSpawn,
        hasKnowledgeBase: !!(n.has_knowledge_base || n.hasKnowledgeBase),
    };
}

async function saveCharacterToDisk(character) {
    try {
        await callTauri('save_character_to_disk', { character: characterToDisk(character) });
        return true;
    } catch (e) {
        console.error('Failed to save character to disk:', e);
        return false;
    }
}

async function loadCharactersFromDisk() {
    // Phase 1: Disk is now the only source of truth for characters.
    try {
        const chars = await callTauri('load_characters_from_disk');
        if (chars && Array.isArray(chars)) {
            return chars.map(normalizeCharacter);
        }
    } catch (e) {
        console.error('Failed to load characters from disk:', e);
    }
    return [];
}

async function deleteCharacterFromDisk(characterId) {
    try {
        await callTauri('delete_character_from_disk', { characterId });
    } catch (e) {
        console.error('Failed to delete character from disk:', e);
    }
}

// === LEGACY chat_histories/*.json helpers (Phase A: deprecated for interactive chat) ===
// Interactive messaging uses conversations/{uuid}/ only. These wrappers log + no-op
// for writes so old call sites cannot corrupt the dual-SoT. Prefer loadConversationMessages.

async function saveChatHistoryToDisk(_ownerId, _history) {
    console.warn('[LocalPersona] saveChatHistoryToDisk is deprecated (Phase A). Use append-only conversations.');
}

async function loadChatHistoryFromDisk(_ownerId) {
    console.warn('[LocalPersona] loadChatHistoryFromDisk is deprecated. Use loadConversationMessages.');
    return [];
}

async function deleteChatHistoryFromDisk(ownerId) {
    // Still allow cleanup of legacy files when deleting a character
    try {
        await callTauri('delete_chat_history_from_disk', { ownerId });
    } catch (e) {
        console.error('Failed to delete legacy chat history:', e);
    }
}

// Helper to save the entire current character list to disk (granular)
async function saveAllCharactersToDisk() {
    // Phase 1: Save all characters to disk (single source of truth)
    for (const char of APP_STATE.characters) {
        try {
            await callTauri('save_character_to_disk', { character: characterToDisk(char) });
        } catch (e) {
            console.error('Failed to save character to disk:', char.id, e);
        }
    }
}

// ==================== VISION HELPERS (MVP) ====================

let pendingImages = [];
let currentServerStatus = null;

async function handleImageUpload() {
    const strip = document.getElementById('imagePreviewStrip');
    if (!strip) return;

    try {
        // Use native Tauri dialog instead of prompt()
        const sourcePath = await callTauri('pick_image_file');
        if (!sourcePath) return;

        const fileName = sourcePath.split(/[\\/]/).pop() || 'image';

        const relativePath = await callTauri('save_uploaded_image', {
            sourcePath: sourcePath,
            originalName: fileName
        });

        if (relativePath) {
            let absPath = '';
            try {
                absPath = await callTauri('resolve_app_path', { relativePath }) || '';
            } catch (_) {}
            const previewUrl = absPath && window.__TAURI__?.core?.convertFileSrc
                ? window.__TAURI__.core.convertFileSrc(absPath)
                : '';
            if (previewUrl) _mediaSrcCache.set(relativePath, previewUrl);
            pendingImages.push({ path: relativePath, name: fileName, previewUrl });
            renderImagePreviews();
        }
    } catch (err) {
        console.error('Failed to pick/save image:', err);
        showToast('Failed to import image: ' + err, 'error');
    }
}

function renderImagePreviews() {
    const strip = document.getElementById('imagePreviewStrip');
    if (!strip) return;

    strip.innerHTML = '';
    strip.classList.toggle('hidden', pendingImages.length === 0);

    pendingImages.forEach((img, index) => {
        const thumb = document.createElement('div');
        thumb.className = 'relative flex-shrink-0 w-12 h-12 rounded-lg overflow-hidden border border-gray-600 bg-surface-light';
        thumb.innerHTML = `
            <img src="${escapeHtmlAttr(resolveMediaSrc(img.path) || img.previewUrl || '')}" class="w-full h-full object-cover" data-img-fallback="strip-clear">
            <button class="absolute -top-1 -right-1 w-5 h-5 bg-red-500 text-white rounded-full text-[10px] flex items-center justify-center">×</button>
            <div class="absolute bottom-0 left-0 right-0 bg-black/60 text-[8px] px-1 truncate">${escapeHtml(img.name)}</div>
        `;
        thumb.querySelector('button').onclick = () => {
            pendingImages.splice(index, 1);
            renderImagePreviews();
        };
        strip.appendChild(thumb);
    });
}

function clearPendingImages() {
    pendingImages = [];
    const strip = document.getElementById('imagePreviewStrip');
    if (strip) {
        strip.innerHTML = '';
        strip.classList.add('hidden');
    }
}

// ==================== END VISION HELPERS ====================

// --- INITIALIZATION ---
async function initApp() {
    // P0-2: install CSP-compatible image fallback delegation before any render.
    try { installImageFallbackHandler(); } catch (e) { console.warn('img fallback init failed', e); }
    // Critical chrome first: Settings/menu must work even if IPC/disk load fails.
    try {
        setupEventListeners();
    } catch (e) {
        console.error('[LocalPersona] setupEventListeners failed:', e);
        // Last-resort Settings binding so the modal can still open.
        try {
            bindClick('settingsBtn', (ev) => { ev.preventDefault(); openSettings(); });
            bindClick('mobileSettingsBtn', (ev) => { ev.preventDefault(); openSettings(); });
            bindClick('quickSettingsWelcomeBtn', (ev) => { ev.preventDefault(); openSettings(); });
            bindClick('closeSettingsBtn', (ev) => { ev.preventDefault(); closeSettings(); });
            bindClick('settingsOverlay', closeSettings);
        } catch (e2) {
            console.error('[LocalPersona] fallback Settings bind failed:', e2);
        }
    }

    // === Professional Architecture (WS4): Load canonical IPC contract for future validation ===
    try {
        const contract = await callTauri('generate_type_schemas');
        if (contract) {
            window.LocalPersonaIPCContract = contract;
            console.log('[LocalPersona] IPC contract loaded for runtime validation (professional contract enforcement).');
        }
    } catch (e) {
        console.warn('[LocalPersona] Could not load IPC contract at startup:', e);
    }

    // Wire 10/10 Diagnostics UI
    try {
        wireDiagnosticsUI();
    } catch (e) {
        console.warn('[LocalPersona] wireDiagnosticsUI failed:', e);
    }

    // === Phase 1: Characters are now loaded exclusively from disk ===
    let savedChars = [];
    try {
        savedChars = await loadCharactersFromDisk();
    } catch (e) {
        console.error('[LocalPersona] loadCharactersFromDisk failed:', e);
        savedChars = [];
    }

    if (savedChars && savedChars.length > 0) {
        APP_STATE.characters = savedChars;
    } else {
        // First run: seed defaults and persist them
        console.log('[LocalPersona] No characters found. Seeding defaults to disk...');
        APP_STATE.characters = JSON.parse(JSON.stringify(DEFAULT_CHARACTERS));
        try {
            await saveAllCharactersToDisk();
        } catch (e) {
            console.warn('[LocalPersona] saveAllCharactersToDisk failed:', e);
        }
    }

    // Normalize character objects (camelCase UI + disk/IPC aliases)
    APP_STATE.characters = APP_STATE.characters.map((char) =>
        normalizeCharacter({
            mode: 'advanced-chat',
            userNickname: '',
            userDescription: '',
            scenario: '',
            writingInstructions: '',
            ...char,
        })
    );

    // Settings can stay on localStorage for now (low risk, user preference)
    const savedSettings = loadFromStorage('settings', null);
    if (savedSettings) {
        APP_STATE.settings = { ...APP_STATE.settings, ...savedSettings };
    }

    // activeCharacterId can stay on localStorage temporarily (UI state)
    APP_STATE.activeCharacterId = loadFromStorage('activeCharacterId', null);

    if (APP_STATE.activeCharacterId && !APP_STATE.characters.find(c => c.id === APP_STATE.activeCharacterId)) {
        APP_STATE.activeCharacterId = APP_STATE.characters.length > 0 ? APP_STATE.characters[0].id : null;
    }

    if (!APP_STATE.activeCharacterId && APP_STATE.characters.length > 0) {
        APP_STATE.activeCharacterId = APP_STATE.characters[0].id;
    }

    try {
        applySettingsToUI();
    } catch (e) {
        console.warn('[LocalPersona] applySettingsToUI at init failed:', e);
    }
    updateConnectionStatus(false);

    // Load real conversation previews before rendering sidebar
    try {
        conversationPreviews = await callTauri('list_conversation_previews');
    } catch (e) {
        console.warn('Could not load conversation previews:', e);
        conversationPreviews = [];
    }

    renderCharacterList();
    renderQuickSwitchCards();
    switchView(APP_STATE.viewMode);

    testConnection(false);
    try {
        if (window.lucide && typeof lucide.createIcons === 'function') {
            lucide.createIcons();
        }
    } catch (e) {
        console.warn('[LocalPersona] lucide.createIcons failed:', e);
    }

    // Smart welcome inference panel on first load (background)
    setTimeout(() => {
        if (typeof updateWelcomeInferencePanel === 'function') {
            updateWelcomeInferencePanel();
        }
    }, 600);

    // Restore persisted llama-server binary path (if any)
    setTimeout(async () => {
        try {
            const savedPath = await callTauri('get_llama_server_path');
            const display = document.getElementById('llamaServerPathDisplay');
            if (savedPath && display) {
                display.textContent = savedPath;
                display.classList.add('text-emerald-400');
            }
        } catch (_) {}
    }, 800);

    console.log('%c[LocalPersona] Frontend initialized. Ready for Tauri IPC integration.', 'color:#a78bfa');
}

// --- RENDER FUNCTIONS (kept from original, slightly adapted) ---
function renderCharacterList() {
    const listEl = document.getElementById('characterList');
    const chars = APP_STATE.characters;

    if (chars.length === 0) {
        listEl.innerHTML = `
            <div class="text-center py-8 px-4">
                <i data-lucide="users" class="w-8 h-8 text-gray-600 mx-auto mb-3"></i>
                <p class="text-gray-500 text-sm">No contacts yet</p>
                <p class="text-gray-600 text-xs mt-1">Add your first simulated contact</p>
            </div>`;
        lucide.createIcons();
        return;
    }

    listEl.innerHTML = chars.map(char => {
        const isActive = char.id === APP_STATE.activeCharacterId;
        const initials = char.name.split(' ').map(w => w[0]).join('').toUpperCase().slice(0, 2);
        const modeIcon = MODE_ICONS[char.mode] || '🎛️';

        let avatarHtml = '';
        if (char.avatarUrl) {
            if (char.avatarUrl.startsWith('avatars/')) {
                // Backend-stored path → render with placeholder, resolve later
                avatarHtml = `
                    <img src="" data-needs-avatar-resolve="true" data-avatar-path="${escapeHtmlAttr(char.avatarUrl)}" alt="" class="w-full h-full object-cover rounded-xl" data-avatar-fallback="sibling" loading="lazy">
                    <span class="w-full h-full rounded-xl flex items-center justify-center text-sm font-bold text-white" style="background:${sanitizeAvatarColor(char.avatarColor)};display:none;">${escapeHtml(initials)}</span>`;
            } else {
                avatarHtml = `<img src="${escapeHtmlAttr(char.avatarUrl)}" alt="" class="w-full h-full object-cover rounded-xl" data-avatar-fallback="sibling" loading="lazy"><span class="w-full h-full rounded-xl flex items-center justify-center text-sm font-bold text-white" style="background:${sanitizeAvatarColor(char.avatarColor)};display:none;">${escapeHtml(initials)}</span>`;
            }
        } else {
            avatarHtml = `<span class="w-full h-full rounded-xl flex items-center justify-center text-sm font-bold text-white" style="background:${sanitizeAvatarColor(char.avatarColor)};">${escapeHtml(initials)}</span>`;
        }

        // Look up real conversation data for this character
        const preview = conversationPreviews.find(p => p.participant_ids.includes(char.id));
        const hasRealData = preview && preview.last_message;
        // M04: Deterministic simulated status based on character id hash + day
        const daySeed = Math.floor(Date.now() / 86400000);
        const charSeed = char.id.split('').reduce((a, c) => a + c.charCodeAt(0), 0);
        const simVal = ((charSeed * 31 + daySeed * 7) % 100) / 100;
        const lastSeen = hasRealData ? 'recent' : (simVal > 0.6 ? 'online' : (simVal > 0.3 ? 'recent' : 'offline'));
        const statusDot = lastSeen === 'online' 
            ? `<span class="absolute bottom-0 right-0 w-3 h-3 bg-emerald-400 border-2 border-surface rounded-full"></span>`
            : lastSeen === 'recent' 
                ? `<span class="absolute bottom-0 right-0 w-3 h-3 bg-yellow-400 border-2 border-surface rounded-full"></span>` 
                : '';

        // Real last message preview or fallback to greeting
        const lastMessagePreview = hasRealData 
            ? preview.last_message.substring(0, 36) + '...'
            : (char.greeting || char.personality || 'Say hello...').substring(0, 36) + '...';

        // Real timestamp or simulated time
        const fakeTime = hasRealData && preview.last_message_time
            ? formatTimestamp(preview.last_message_time)
            : lastSeen === 'online' ? 'now' : (lastSeen === 'recent' ? '2h' : 'yest');

        // Message count badge (not "unread" — we do not track read state)
        const unread = hasRealData && preview.message_count > 0
            ? `<span class="px-1.5 py-px text-[10px] leading-none bg-surface-light text-gray-400 rounded-full font-medium" title="Messages in conversation">${preview.message_count > 99 ? '99+' : preview.message_count}</span>`
            : '';

        return `
            <div class="character-list-item flex items-center gap-3 px-3 py-2.5 rounded-xl cursor-pointer transition-all duration-200 group ${isActive ? 'bg-accent/20 border border-accent/30' : 'hover:bg-surface-hover'}"
                 data-character-id="${char.id}"
                 data-action="select">
                
                <div class="w-11 h-11 rounded-full flex-shrink-0 overflow-hidden shadow-sm relative flex-none ring-1 ring-gray-700/50">
                    ${avatarHtml}
                    ${statusDot}
                </div>

                <div class="flex-1 min-w-0 overflow-hidden">
                    <div class="flex justify-between items-baseline">
                        <p class="font-medium text-[15px] text-white truncate">${escapeHtml(char.name)}</p>
                        <span class="text-[10px] text-gray-500 flex-shrink-0 ml-2">${fakeTime}</span>
                    </div>
                    
                    <div class="flex items-center justify-between mt-px">
                        <p class="text-xs text-gray-400 truncate pr-3">${escapeHtml(lastMessagePreview)}</p>
                        ${unread}
                    </div>
                </div>

                <button class="p-1 rounded-lg hover:bg-surface-hover opacity-0 group-hover:opacity-100 transition-all flex-shrink-0" 
                        data-action="edit" data-character-id="${char.id}" title="Edit contact">
                    <i data-lucide="pencil" class="w-3.5 h-3.5 text-gray-400"></i>
                </button>
            </div>`;
    }).join('');

    lucide.createIcons();

    // Resolve backend-stored avatar paths asynchronously
    resolvePendingAvatars(listEl);

    listEl.querySelectorAll('[data-action="select"]').forEach(el => {
        el.addEventListener('click', (e) => {
            if (e.target.closest('[data-action="edit"]')) return;
            selectCharacter(el.dataset.characterId);
        });
    });

    listEl.querySelectorAll('[data-action="edit"]').forEach(btn => {
        btn.addEventListener('click', (e) => {
            e.stopPropagation();
            openCharacterEditor(btn.dataset.characterId);
        });
    });
}

function renderQuickSwitchCards() {
    const container = document.getElementById('quickSwitchCards');
    const chars = APP_STATE.characters;

    if (chars.length === 0) {
        container.innerHTML = '<p class="text-gray-600 text-sm py-2">No characters available</p>';
        return;
    }

    container.innerHTML = chars.map(char => {
        const isActive = char.id === APP_STATE.activeCharacterId;
        const initials = char.name.split(' ').map(w => w[0]).join('').toUpperCase().slice(0, 2);
        const modeIcon = MODE_ICONS[char.mode] || '🎛️';

        let avatarHtml = '';
        if (char.avatarUrl) {
            if (char.avatarUrl.startsWith('avatars/')) {
                avatarHtml = `
                    <img src="" data-needs-avatar-resolve="true" data-avatar-path="${escapeHtmlAttr(char.avatarUrl)}" alt="" class="w-full h-full object-cover" data-avatar-fallback="sibling" loading="lazy">
                    <span class="w-full h-full flex items-center justify-center text-[10px] font-bold text-white" style="background:${sanitizeAvatarColor(char.avatarColor)};display:none;">${escapeHtml(initials)}</span>`;
            } else {
                avatarHtml = `<img src="${escapeHtmlAttr(char.avatarUrl)}" alt="" class="w-full h-full object-cover" data-avatar-fallback="sibling" loading="lazy"><span class="w-full h-full flex items-center justify-center text-[10px] font-bold text-white" style="background:${sanitizeAvatarColor(char.avatarColor)};display:none;">${escapeHtml(initials)}</span>`;
            }
        } else {
            avatarHtml = `<span class="w-full h-full flex items-center justify-center text-[10px] font-bold text-white" style="background:${sanitizeAvatarColor(char.avatarColor)};">${escapeHtml(initials)}</span>`;
        }

        return `
            <button class="quick-switch-card flex items-center gap-2 px-3 py-2 rounded-xl transition-all duration-200 flex-shrink-0 ${isActive ? 'bg-accent/25 border border-accent/40 shadow-md shadow-accent/10' : 'bg-surface-light hover:bg-surface-hover border border-transparent'}"
                    data-character-id="${char.id}"
                    data-action="quick-select">
                <div class="w-7 h-7 rounded-lg overflow-hidden flex-shrink-0 shadow-sm">
                    ${avatarHtml}
                </div>
                <span class="text-xs font-medium truncate max-w-[80px] ${isActive ? 'text-white' : 'text-gray-300'}">${escapeHtml(char.name)}</span>
                <span class="text-[10px]">${modeIcon}</span>
            </button>`;
    }).join('');

    // Resolve backend-stored avatars
    resolvePendingAvatars(container);

    container.querySelectorAll('[data-action="quick-select"]').forEach(btn => {
        btn.addEventListener('click', () => {
            selectCharacter(btn.dataset.characterId);
        });
    });
}

async function updateChatView() {
    const welcomeState = document.getElementById('welcomeState');
    const messagesContainer = document.getElementById('messagesContainer');
    const chatInput = document.getElementById('chatInput');
    const sendBtn = document.getElementById('sendBtn');
    const regenerateBtn = document.getElementById('regenerateBtn');
    const mobileTitle = document.getElementById('mobileTitle');

    const activeChar = APP_STATE.characters.find(c => c.id === APP_STATE.activeCharacterId);

    if (!activeChar) {
        welcomeState.classList.remove('hidden');
        welcomeState.style.display = 'flex';
        messagesContainer.classList.add('hidden');
        messagesContainer.innerHTML = '';
        chatInput.disabled = true;
        sendBtn.disabled = true;
        regenerateBtn.classList.add('hidden');
        mobileTitle.textContent = 'LocalPersona';
        chatInput.placeholder = 'Select a character to start chatting...';

        // NEW: Show dynamic inference quick-start panel on welcome screen
        setTimeout(() => {
            updateWelcomeInferencePanel();
        }, 50);
        return;
    }

    welcomeState.classList.add('hidden');
    welcomeState.style.display = 'none';
    messagesContainer.classList.remove('hidden');
    chatInput.disabled = false;
    sendBtn.disabled = APP_STATE.isGenerating;
    mobileTitle.textContent = activeChar.name;

    // Status is shown in the sidebar list

    chatInput.placeholder = `Message ${activeChar.name}...`;
    chatInput.focus();

    const userLabel = document.querySelector('.voice-user-label');
    const botLabel = document.querySelector('.voice-bot-label');
    if (userLabel) userLabel.textContent = activeChar.userNickname || 'You';
    if (botLabel) botLabel.textContent = activeChar.name;

    // Phase 1: Load from the append-only conversation system
    const messages = currentConversationId 
        ? await loadConversationMessages(true)   // Phase 2: bounded/token-budgeted by default
        : [];

    renderMessages(messages, activeChar);
    updateContextUsagePill();

    // Show "Load earlier messages" affordance when the backend signals there is more history
    updateLoadEarlierButton(window.currentConversationHasMore);

    const hasAiMessages = messages.some(m => m.role === 'assistant');
    if (hasAiMessages && !APP_STATE.isGenerating) {
        regenerateBtn.classList.remove('hidden');
    } else if (!APP_STATE.isGenerating) {
        regenerateBtn.classList.add('hidden');
    }
}

// ===================================================================
// HIGH-PERFORMANCE INCREMENTAL RENDERING (from adversarial audit)
// ===================================================================

function appendMessageToDOM(message) {
    if (renderedMessageIds.has(message.id)) return;

    const container = document.getElementById('messagesContainer');
    if (!container) return;

    const activeChar = APP_STATE.characters.find(c => c.id === APP_STATE.activeCharacterId);
    if (!activeChar) return;

    const html = renderSingleMessage(message, activeChar);
    container.insertAdjacentHTML('beforeend', html);
    renderedMessageIds.add(message.id);

    // Attach action listeners to the newly added element
    const newEl = container.lastElementChild;
    if (newEl) attachMessageActionListeners(newEl);

    scrollToBottom();
}

function renderSingleMessage(msg, character) {
    const time = msg.timestamp ? new Date(msg.timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : '';
    const initials = character.name.split(' ').map(w => w[0]).join('').toUpperCase().slice(0, 2);
    const userName = character.userNickname || 'You';

    if (msg.role === 'user') {
        // WhatsApp-style right bubble (user)
        return `
            <div class="flex justify-end animate-fadeIn message-group" data-message-id="${msg.id}">
                <div class="max-w-[78%] bg-[#005c4b] text-white rounded-2xl rounded-tr-md px-3.5 py-2 shadow-sm">
                    <p class="text-[14.5px] leading-snug whitespace-pre-wrap">${escapeHtml(msg.content)}</p>
                    <span class="block text-right text-[10px] text-white/60 mt-1">${time}</span>
                </div>
            </div>`;
    } else if (msg.role === 'narrator') {
        return `
            <div class="flex justify-center my-2 animate-fadeIn message-group" data-message-id="${msg.id}">
                <div class="max-w-[70%] bg-gray-800/60 text-gray-300 text-xs italic px-3 py-1.5 rounded-full text-center">
                    ${formatMessageContent(msg.content)}
                </div>
            </div>`;
    } else {
        // WhatsApp-style left bubble (AI)
        return `
            <div class="flex items-start gap-2 animate-fadeIn message-group" data-message-id="${msg.id}">
                <div class="w-7 h-7 rounded-full flex-shrink-0 overflow-hidden mt-0.5" style="background:${sanitizeAvatarColor(character.avatarColor)};">
                    <span class="w-full h-full flex items-center justify-center text-[10px] font-bold text-white">${escapeHtml(initials)}</span>
                </div>
                <div class="max-w-[78%] bg-[#1f2c34] text-gray-100 rounded-2xl rounded-tl-md px-3.5 py-2 shadow-sm">
                    <p class="text-[14.5px] leading-snug whitespace-pre-wrap">${formatMessageContent(msg.content)}</p>
                    <span class="block text-[10px] text-gray-400 mt-1">${escapeHtml(msg.speaker_name || character.name)} · ${time}</span>
                </div>
            </div>`;
    }
}

function attachMessageActionListeners(element) {
    element.querySelectorAll('[data-action="copy-message"]').forEach(btn => {
        btn.onclick = () => {
            const id = btn.dataset.id || btn.closest('.message-group')?.dataset.messageId;
            if (id) copyMessageById(id);
        };
    });
    // R3: Edit actions hidden for RC (no false affordance). Copy remains.
    element.querySelectorAll('[data-action="edit-message"]').forEach(btn => {
        btn.classList.add('hidden');
        btn.onclick = null;
    });
}

// Play voice for a character response using the configured TTS backend
async function playCharacterVoice(text, character) {
    if (!text || !character) return;

    try {
        const settings = APP_STATE.settings || {};
        const ttsEndpoint = settings.ttsEndpoint || 'http://localhost:8081/v1/audio/speech';

        const voiceId = character.voicePreset || character.voiceMode || 'default';

        const audioBytes = await callTauri('generate_speech', {
            text: text,
            voice: voiceId,
            ttsEndpoint: ttsEndpoint,
        });

        if (!audioBytes || audioBytes.length === 0) {
            console.warn('No audio returned from TTS');
            return;
        }

        // Convert the bytes to a playable audio blob
        const blob = new Blob([new Uint8Array(audioBytes)], { type: 'audio/wav' });
        const url = URL.createObjectURL(blob);

        const audio = new Audio(url);
        audio.play().catch(err => console.warn('Audio playback failed:', err));

        // Clean up URL after playback
        audio.onended = () => URL.revokeObjectURL(url);
    } catch (e) {
        console.warn('Voice playback error:', e);
        // Don't block the chat if TTS fails
    }
}

async function copyMessageById(messageId) {
    const msgs = await callTauri('load_all_messages', { conversationId: currentConversationId });
    const msg = msgs?.find(m => m.id === messageId);
    if (msg) navigator.clipboard.writeText(msg.content);
}

function renderMessages(history, character) {
    const container = document.getElementById('messagesContainer');
    // Full re-render: rebuild id set so "load earlier" / switches stay consistent.
    renderedMessageIds = new Set();
    const initials = character.name.split(' ').map(w => w[0]).join('').toUpperCase().slice(0, 2);
    const userName = character.userNickname || 'You';

    if (history.length === 0) {
        let greetingAvatar = '';
        if (character.avatarUrl && character.avatarUrl.startsWith('avatars/')) {
            greetingAvatar = `<img src="" data-needs-avatar-resolve="true" data-avatar-path="${escapeHtmlAttr(character.avatarUrl)}" alt="" class="w-full h-full object-cover rounded-full" data-avatar-fallback="parent-text" data-fallback-text="${escapeHtmlAttr(initials)}" loading="lazy">`;
        } else if (character.avatarUrl) {
            greetingAvatar = `<img src="${escapeHtmlAttr(character.avatarUrl)}" alt="" class="w-full h-full object-cover rounded-full" data-avatar-fallback="parent-text" data-fallback-text="${escapeHtmlAttr(initials)}" loading="lazy">`;
        } else {
            greetingAvatar = `<span class="text-xs font-bold text-white">${escapeHtml(initials)}</span>`;
        }

        container.innerHTML = `
            <div class="flex items-start gap-3 animate-fadeIn message-group">
                <div class="w-8 h-8 rounded-full flex-shrink-0 overflow-hidden shadow-md flex items-center justify-center" style="background:${sanitizeAvatarColor(character.avatarColor)};">
                    ${greetingAvatar}
                </div>
                <div class="bg-chat-ai rounded-2xl rounded-tl-md px-4 py-3 max-w-[80%] shadow-sm">
                    <p class="text-sm text-gray-200 message-content whitespace-pre-wrap">${escapeHtml(character.greeting)}</p>
                    <span class="text-[10px] text-gray-600 mt-2 block">${character.name}</span>
                </div>
            </div>`;
        resolvePendingAvatars(container);
        return;
    }

    container.innerHTML = history.map((msg, index) => {
        const time = msg.timestamp ? new Date(msg.timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : '';
        const voice = msg.voice || (msg.role === 'user' ? 'user' : 'bot');

        if (voice === 'narrator') {
            return `
                <div class="narrator-message my-3 animate-fadeIn message-group" data-message-index="${index}">
                    <div class="narrator-bubble">
                        <p class="text-sm message-content whitespace-pre-wrap">${formatMessageContent(msg.content)}</p>
                        <span class="text-[10px] text-gray-600 mt-1 block">🎭 Narrator · ${time}</span>
                    </div>
                </div>`;
        }

        if (voice === 'bot' && msg.role === 'assistant') {
            return `
                <div class="flex items-start gap-3 animate-fadeIn message-group" data-message-index="${index}">
                    <div class="w-8 h-8 rounded-full flex-shrink-0 overflow-hidden shadow-md flex items-center justify-center" style="background:${sanitizeAvatarColor(character.avatarColor)};">
                        ${character.avatarUrl && character.avatarUrl.startsWith('avatars/')
                            ? `<img src="" data-needs-avatar-resolve="true" data-avatar-path="${escapeHtmlAttr(character.avatarUrl)}" alt="" class="w-full h-full object-cover rounded-full" data-avatar-fallback="parent-text" data-fallback-text="${escapeHtmlAttr(initials)}" loading="lazy">`
                            : character.avatarUrl
                                ? `<img src="${escapeHtmlAttr(character.avatarUrl)}" alt="" class="w-full h-full object-cover rounded-full" data-avatar-fallback="parent-text" data-fallback-text="${escapeHtmlAttr(initials)}" loading="lazy">`
                                : `<span class="text-xs font-bold text-white">${escapeHtml(initials)}</span>`}
                    </div>
                    <div class="bg-chat-ai rounded-2xl rounded-tl-md px-4 py-3 max-w-[80%] shadow-sm relative">
                        <p class="text-sm text-gray-200 message-content whitespace-pre-wrap">${formatMessageContent(msg.content)}</p>
                        ${renderMessageImages(msg.images)}
                        <span class="text-[10px] text-gray-600 mt-1 block">${character.name} · ${time}</span>
                        <div class="message-actions absolute -bottom-6 left-0 flex gap-1">
                            <button class="p-1 rounded hover:bg-surface-hover transition-colors" data-action="copy-message" data-index="${index}" title="Copy">
                                <i data-lucide="copy" class="w-3 h-3 text-gray-500"></i>
                            </button>
                        </div>
                    </div>
                </div>`;
        }

        const isUser = msg.role === 'user';

        if (isUser) {
            return `
                <div class="flex items-start gap-3 justify-end animate-fadeIn message-group" data-message-index="${index}">
                    <div class="bg-chat-user rounded-2xl rounded-tr-md px-4 py-3 max-w-[80%] shadow-sm relative">
                        <p class="text-sm text-gray-100 message-content whitespace-pre-wrap">${escapeHtml(msg.content)}</p>
                        ${renderMessageImages(msg.images)}
                        <span class="text-[10px] text-gray-500 mt-1 block text-right">${userName} · ${time}</span>
                        <div class="message-actions absolute -bottom-6 right-0 flex gap-1">
                            <button class="p-1 rounded hover:bg-surface-hover transition-colors" data-action="copy-message" data-index="${index}" title="Copy">
                                <i data-lucide="copy" class="w-3 h-3 text-gray-500"></i>
                            </button>
                        </div>
                    </div>
                    <div class="w-8 h-8 rounded-full bg-accent/40 flex-shrink-0 flex items-center justify-center text-xs font-bold text-white shadow-md">${userName.slice(0, 2)}</div>
                </div>`;
        } else {
            return `
                <div class="flex items-start gap-3 animate-fadeIn message-group" data-message-index="${index}">
                    <div class="w-8 h-8 rounded-full flex-shrink-0 overflow-hidden shadow-md flex items-center justify-center" style="background:${sanitizeAvatarColor(character.avatarColor)};">
                        ${character.avatarUrl && character.avatarUrl.startsWith('avatars/')
                            ? `<img src="" data-needs-avatar-resolve="true" data-avatar-path="${escapeHtmlAttr(character.avatarUrl)}" alt="" class="w-full h-full object-cover rounded-full" data-avatar-fallback="parent-text" data-fallback-text="${escapeHtmlAttr(initials)}" loading="lazy">`
                            : character.avatarUrl
                                ? `<img src="${escapeHtmlAttr(character.avatarUrl)}" alt="" class="w-full h-full object-cover rounded-full" data-avatar-fallback="parent-text" data-fallback-text="${escapeHtmlAttr(initials)}" loading="lazy">`
                                : `<span class="text-xs font-bold text-white">${escapeHtml(initials)}</span>`}
                    </div>
                    <div class="bg-chat-ai rounded-2xl rounded-tl-md px-4 py-3 max-w-[80%] shadow-sm relative">
                        <p class="text-sm text-gray-200 message-content whitespace-pre-wrap">${formatMessageContent(msg.content)}</p>
                        ${renderMessageImages(msg.images)}
                        <span class="text-[10px] text-gray-600 mt-1 block">${character.name} · ${time}</span>
                        <div class="message-actions absolute -bottom-6 left-0 flex gap-1">
                            <button class="p-1 rounded hover:bg-surface-hover transition-colors" data-action="copy-message" data-index="${index}" title="Copy">
                                <i data-lucide="copy" class="w-3 h-3 text-gray-500"></i>
                            </button>
                            <button class="p-1 rounded hover:bg-surface-hover transition-colors" data-action="regenerate-from" data-index="${index}" title="Regenerate from here">
                                <i data-lucide="refresh-cw" class="w-3 h-3 text-gray-500"></i>
                            </button>
                        </div>
                    </div>
                </div>`;
        }
    }).join('');

    lucide.createIcons();

    // Resolve backend-stored avatars in chat messages
    resolvePendingAvatars(container);

    container.querySelectorAll('[data-action="copy-message"]').forEach(btn => {
        btn.addEventListener('click', () => copyMessage(parseInt(btn.dataset.index)));
    });
    // Edit UI removed for RC (R3)
    container.querySelectorAll('[data-action="regenerate-from"]').forEach(btn => {
        btn.addEventListener('click', () => regenerateFrom(parseInt(btn.dataset.index)));
    });

    scrollToBottom();
}

function renderMessageImages(images) {
    if (!images || images.length === 0) return '';
    let html = '<div class="message-images">';
    images.forEach(img => {
        if (!img.path) return;
        const cached = resolveMediaSrc(img.path);
        const pathAttr = escapeHtmlAttr(img.path);
        html += `<img src="${escapeHtmlAttr(cached)}" data-media-path="${pathAttr}" alt="Attached image" class="max-w-full rounded-lg">`;
        if (!cached) resolveMediaSrcAsync(img.path);
    });
    html += '</div>';
    return html;
}

/** Cache: app-relative path → convertFileSrc URL */
const _mediaSrcCache = new Map();

/** Resolve app-relative image paths for webview display (Tauri convertFileSrc needs absolute paths). */
function resolveMediaSrc(relPath) {
    if (!relPath) return '';
    if (
        relPath.startsWith('data:')
        || relPath.startsWith('http://')
        || relPath.startsWith('https://')
        || relPath.startsWith('asset:')
        || relPath.startsWith('file://')
        || relPath.startsWith('/')
    ) {
        try {
            if (relPath.startsWith('/') && window.__TAURI__?.core?.convertFileSrc) {
                return window.__TAURI__.core.convertFileSrc(relPath);
            }
        } catch (_) {}
        return relPath;
    }
    if (_mediaSrcCache.has(relPath)) return _mediaSrcCache.get(relPath);
    // Kick off async resolve; return empty for now (img will get src set later).
    resolveMediaSrcAsync(relPath).catch(() => {});
    return '';
}

async function resolveMediaSrcAsync(relPath) {
    if (!relPath || _mediaSrcCache.has(relPath)) return _mediaSrcCache.get(relPath) || '';
    try {
        const abs = await callTauri('resolve_app_path', { relativePath: relPath });
        if (abs && window.__TAURI__?.core?.convertFileSrc) {
            const url = window.__TAURI__.core.convertFileSrc(abs);
            _mediaSrcCache.set(relPath, url);
            const esc = (typeof CSS !== 'undefined' && CSS.escape)
                ? CSS.escape(relPath)
                : relPath.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
            document.querySelectorAll(`img[data-media-path="${esc}"]`).forEach((img) => {
                img.src = url;
            });
            return url;
        }
    } catch (e) {
        console.warn('resolveMediaSrcAsync failed:', relPath, e);
    }
    return '';
}

function formatMessageContent(content) {
    let formatted = escapeHtml(content);
    formatted = formatted.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
    formatted = formatted.replace(/\*(.+?)\*/g, '<em>$1</em>');
    formatted = formatted.replace(/`(.+?)`/g, '<code>$1</code>');
    formatted = formatted.replace(/\n\n/g, '</p><p>');
    formatted = '<p>' + formatted + '</p>';
    formatted = formatted.replace(/\n/g, '<br>');
    formatted = formatted.replace(/<p><\/p>/g, '');
    return formatted;
}

function escapeHtml(str) {
    const div = document.createElement('div');
    div.textContent = str;
    return div.innerHTML;
}

// P0-2 hardening: allowlist for avatar colors (prevents CSS/style injection via imported characters).
function sanitizeAvatarColor(color) {
    if (typeof color === 'string' && /^#[0-9a-fA-F]{6}$/.test(color)) return color;
    return '#7c3aed';
}

// P0-2 hardening: attribute-safe escaping (escapeHtml does not escape single quotes).
function escapeHtmlAttr(str) {
    return escapeHtml(String(str ?? '')).replace(/'/g, '&#39;').replace(/"/g, '&quot;');
}

// P0-2 hardening: delegated image-error handling (replaces all inline handlers).
// Works under strict CSP (no 'unsafe-inline' in script-src). Modes:
// - data-img-fallback="strip-clear": clear src + add bg class (preview thumbnails)
// - data-avatar-fallback="sibling": hide img, show next sibling fallback span
// - data-avatar-fallback="parent-text": hide img, set parent textContent safely (no inline JS)
function installImageFallbackHandler() {
    if (window.__lpImgFallbackInstalled) return;
    window.__lpImgFallbackInstalled = true;
    document.addEventListener('error', (e) => {
        const t = e.target;
        if (!t || t.tagName !== 'IMG') return;
        if (t.dataset.imgFallback === 'strip-clear') {
            t.src = '';
            t.classList.add('bg-gray-700');
        } else if (t.dataset.avatarFallback === 'sibling') {
            t.style.display = 'none';
            if (t.nextElementSibling) t.nextElementSibling.style.display = 'flex';
        } else if (t.dataset.avatarFallback === 'parent-text') {
            const txt = t.dataset.fallbackText || '';
            t.style.display = 'none';
            if (t.parentElement) t.parentElement.textContent = txt;
        }
    }, true);
}

function buildSystemPrompt(character) {
    if (character.systemPrompt && character.systemPrompt.trim()) {
        return character.systemPrompt.trim();
    }

    let prompt = '';

    switch (character.mode) {
        case 'adventure':
            prompt += 'You are narrating an interactive adventure. Describe the world vividly, present choices, and respond to the player\'s actions. Use second person ("you") for the player\'s perspective. Make the adventure exciting with unexpected twists and meaningful choices. Use asterisks for actions and narrative description.\n\n';
            break;
        case 'story':
            prompt += 'You are co-writing a story. Write in a literary style with rich prose, vivid descriptions, and compelling narrative. Continue the story naturally based on what has been written so far. Maintain consistent tone, pacing, and character voices.\n\n';
            break;
        case 'character-gen':
            prompt += 'You are a character designer. Help create vivid, well-rounded characters with detailed personalities, backgrounds, motivations, and quirks. Ask clarifying questions when needed and provide rich character descriptions.\n\n';
            break;
        case 'advanced-chat':
        default:
            prompt += 'You are a helpful and engaging conversational partner. Be natural, thoughtful, and responsive.\n\n';
            break;
    }

    if (character.personality) {
        prompt += `Your character: ${character.personality}\n\n`;
    }
    if (character.userDescription) {
        prompt += `The user's character: ${character.userDescription}\n\n`;
    }
    if (character.name) {
        prompt += `Your name: ${character.name}\n`;
    }
    if (character.userNickname) {
        prompt += `The user's name: ${character.userNickname}\n`;
    }
    if (character.name || character.userNickname) {
        prompt += '\n';
    }
    if (character.scenario) {
        prompt += `Scenario & Lore: ${character.scenario}\n\n`;
    }
    if (character.writingInstructions) {
        prompt += `Writing instructions: ${character.writingInstructions}\n`;
    }

    return prompt.trim();
}

APP_STATE.activeVoice = 'user';
APP_STATE.guidance = {
    nextAction: '',
    writingInstructions: '',
};

// --- VISION STATE (MVP) ---
function updateAvatarPreview(url) {
    const preview = document.getElementById('avatarPreview');
    if (!preview) return;
    const cameraIcon = preview.querySelector('i[data-lucide="camera"]');
    const existingImg = preview.querySelector('img');
    if (existingImg) existingImg.remove();

    if (url && (url.startsWith('data:') || url.startsWith('http'))) {
        const img = document.createElement('img');
        img.src = url;
        img.alt = 'Avatar preview';
        img.style.width = '100%';
        img.style.height = '100%';
        img.style.objectFit = 'cover';
        img.onerror = () => {
            img.remove();
            if (cameraIcon) cameraIcon.style.display = '';
            document.getElementById('clearAvatarBtn').classList.add('hidden');
        };
        if (cameraIcon) cameraIcon.style.display = 'none';
        preview.appendChild(img);
        document.getElementById('clearAvatarBtn').classList.remove('hidden');
    } else {
        if (cameraIcon) cameraIcon.style.display = '';
        document.getElementById('clearAvatarBtn').classList.add('hidden');
    }
}

/**
 * Resolves an avatar value (data URL, http, or "avatars/xxx.png" path) into a usable src.
 */
async function resolveAvatarSrc(value) {
    if (!value) return null;
    if (value.startsWith('data:') || value.startsWith('http')) {
        return value;
    }
    if (value.startsWith('avatars/')) {
        const dataUrl = await callTauri('get_character_avatar', { relativePath: value });
        return dataUrl || null;
    }
    return null;
}

/**
 * After rendering HTML that contains avatars, this function finds images
 * that are using backend paths and resolves them asynchronously.
 */
async function resolvePendingAvatars(container) {
    if (!container) return;

    const images = container.querySelectorAll('img[data-needs-avatar-resolve="true"]');
    for (const img of images) {
        const path = img.dataset.avatarPath;
        if (path) {
            const resolved = await resolveAvatarSrc(path);
            if (resolved) {
                img.src = resolved;
                img.removeAttribute('data-needs-avatar-resolve');
                img.removeAttribute('data-avatar-path');
            } else {
                // fallback to initials
                img.style.display = 'none';
                const fallback = img.nextElementSibling;
                if (fallback) fallback.style.display = 'flex';
            }
        }
    }
}

// --- EXPORT / IMPORT (kept as-is for now) ---
async function exportAllCharacters() {
    const json = JSON.stringify(APP_STATE.characters, null, 2);
    const result = await callTauri('export_characters', { charactersJson: json });
    if (result === null) {
        // user cancelled or error — callTauri already logs
    } else {
        // Success is handled on Rust side (native dialog)
    }
}

async function exportSingleCharacter(char) {
    const json = JSON.stringify(char, null, 2);
    await callTauri('export_characters', { charactersJson: json });
}

/** Accept epoch millis or seconds (backend uses millis). */
function formatTimestamp(epochValue) {
    if (epochValue == null || !Number.isFinite(Number(epochValue))) return '';
    let epochMs = Number(epochValue);
    // Values below year ~2001 in ms are almost certainly seconds
    if (epochMs < 1e12) epochMs = epochMs * 1000;
    const diffSecs = (Date.now() - epochMs) / 1000;
    if (diffSecs < 60) return 'now';
    if (diffSecs < 3600) return Math.floor(diffSecs / 60) + 'm';
    if (diffSecs < 86400) return Math.floor(diffSecs / 3600) + 'h';
    if (diffSecs < 604800) return Math.floor(diffSecs / 86400) + 'd';
    const d = new Date(epochMs);
    return d.toLocaleDateString([], { month: 'short', day: 'numeric' });
}

function scrollToBottom() {
    const chatMessages = document.getElementById('chatMessages');
    requestAnimationFrame(() => {
        chatMessages.scrollTop = chatMessages.scrollHeight;
    });
}

// --- VIEW / NAVIGATION ---
/**
 * Switch the main pane between welcome and chat.
 * 'chat' and 'config' both show the conversation area (config is a legacy alias).
 * Always refreshes via updateChatView so the UI matches APP_STATE.
 */
function switchView(mode) {
    const next = mode || APP_STATE.viewMode || 'welcome';
    APP_STATE.viewMode = next === 'config' ? 'chat' : next;
    // Fire-and-forget is fine for UI; callers that need freshness await updateChatView.
    updateChatView().catch((e) => console.warn('[LocalPersona] switchView update failed:', e));
}

// --- CHARACTER MANAGEMENT ---
let _selectGeneration = 0;

/** Bumps when a generation starts; selectCharacter + send finally check this to avoid wrong-chat UI. */
let _inflightGenerationToken = 0;

async function selectCharacter(charId) {
    // Switching contacts mid-generation: block until the in-flight request finishes
    // so the reply cannot land on a different conversation in the UI.
    if (APP_STATE.isGenerating) {
        showToast('Wait for the current reply to finish before switching contacts.', 'warning');
        return;
    }

    const generation = ++_selectGeneration;
    APP_STATE.activeCharacterId = charId;
    APP_STATE.activeVoice = 'user';
    saveToStorage('activeCharacterId', charId);

    // Reset conversation render state before loading the new contact
    renderedMessageIds = new Set();
    currentConversationId = null;
    window.currentConversationHasMore = false;

    renderCharacterList();
    renderQuickSwitchCards();

    const activeChar = APP_STATE.characters.find(c => c.id === charId);
    const convId = await getOrCreateConversationForCharacter(charId, activeChar?.name || 'Contact');

    // Stale selection: user switched again while we were awaiting
    if (generation !== _selectGeneration) return;

    if (!convId) {
        console.warn('Could not create conversation for character');
    }

    APP_STATE.viewMode = 'chat';
    await updateChatView();
    if (generation !== _selectGeneration) return;

    closeMobileSidebar();

    const card = document.querySelector(`[data-character-id="${charId}"][data-action="quick-select"]`);
    if (card) card.scrollIntoView({ behavior: 'smooth', block: 'nearest', inline: 'center' });
}

function openCharacterEditor(charId = null) {
    const modal = document.getElementById('characterEditorModal');
    const title = document.getElementById('charEditorTitle');
    const deleteBtn = document.getElementById('deleteCharBtn');
    const nameInput = document.getElementById('charNameInput');
    const userNicknameInput = document.getElementById('charUserNicknameInput');
    const avatarUrlInput = document.getElementById('charAvatarUrlInput');
    const personalityInput = document.getElementById('charPersonalityInput');
    const userDescInput = document.getElementById('charUserDescInput');
    const scenarioInput = document.getElementById('charScenarioInput');
    const writingInstructionsInput = document.getElementById('charWritingInstructionsInput');
    const systemPromptInput = document.getElementById('charSystemPromptInput');
    const greetingInput = document.getElementById('charGreetingInput');
    const colorPicker = document.getElementById('colorPicker');

    nameInput.value = '';
    userNicknameInput.value = '';
    avatarUrlInput.value = '';
    personalityInput.value = '';
    userDescInput.value = '';
    scenarioInput.value = '';
    writingInstructionsInput.value = '';
    systemPromptInput.value = '';
    greetingInput.value = '';
    document.getElementById('saveCharBtn').dataset.editingId = '';

    const writingSection = document.getElementById('writingInstructionsSection');
    const systemSection = document.getElementById('systemPromptSection');
    if (writingSection) writingSection.classList.add('hidden');
    if (systemSection) systemSection.classList.add('hidden');

    title.textContent = 'Add New Contact';

    colorPicker.querySelectorAll('button').forEach((btn, i) => {
        if (i === 0) btn.classList.add('ring-2', 'ring-white/20', 'ring-offset-2', 'ring-offset-surface');
        else btn.classList.remove('ring-2', 'ring-white/20', 'ring-offset-2', 'ring-offset-surface');
    });

    document.querySelectorAll('#modeSelector .mode-card').forEach(btn => {
        btn.classList.remove('border-accent', 'bg-accent/15', 'text-accent-light');
        btn.classList.add('border-transparent', 'bg-surface-light', 'text-gray-400');
    });
    const defaultModeBtn = document.querySelector('#modeSelector [data-mode="advanced-chat"]');
    if (defaultModeBtn) {
        defaultModeBtn.classList.add('border-accent', 'bg-accent/15', 'text-accent-light');
        defaultModeBtn.classList.remove('border-transparent', 'bg-surface-light', 'text-gray-400');
    }

    // === Voice UI initialization ===
    setupVoiceEditorUI();

    if (charId) {
        const char = APP_STATE.characters.find(c => c.id === charId);
        if (char) {
            title.textContent = 'Edit Contact';
            deleteBtn.classList.remove('hidden');
            document.getElementById('saveCharBtn').dataset.editingId = charId;
            nameInput.value = char.name;
            userNicknameInput.value = char.userNickname || '';
            avatarUrlInput.value = char.avatarUrl || '';
            personalityInput.value = char.personality;
            userDescInput.value = char.userDescription || '';
            scenarioInput.value = char.scenario || '';
            writingInstructionsInput.value = char.writingInstructions || '';
            systemPromptInput.value = char.systemPrompt || '';
            greetingInput.value = char.greeting;

            const mode = char.mode || 'advanced-chat';
            document.querySelectorAll('#modeSelector .mode-card').forEach(btn => {
                btn.classList.remove('border-accent', 'bg-accent/15', 'text-accent-light');
                btn.classList.add('border-transparent', 'bg-surface-light', 'text-gray-400');
                if (btn.dataset.mode === mode) {
                    btn.classList.add('border-accent', 'bg-accent/15', 'text-accent-light');
                    btn.classList.remove('border-transparent', 'bg-surface-light', 'text-gray-400');
                }
            });

            if (char.writingInstructions) writingSection.classList.remove('hidden');
            if (char.systemPrompt) systemSection.classList.remove('hidden');

            // Load voice settings
            const voiceMode = char.voiceMode || 'none';
            document.querySelectorAll('#voiceModeSelector .voice-mode-btn').forEach(btn => {
                btn.classList.remove('border-accent', 'bg-accent/15', 'text-accent-light');
                if (btn.dataset.voiceMode === voiceMode) {
                    btn.classList.add('border-accent', 'bg-accent/15', 'text-accent-light');
                }
            });

            const presetSection = document.getElementById('voicePresetSection');
            const customSection = document.getElementById('voiceCustomSection');
            if (presetSection) presetSection.classList.toggle('hidden', voiceMode !== 'preset');
            if (customSection) customSection.classList.toggle('hidden', voiceMode !== 'custom');

            const presetSelect = document.getElementById('charVoicePresetInput');
            if (presetSelect && char.voicePreset) {
                presetSelect.value = char.voicePreset;
            }

            if (char.voiceSamplePath) {
                const status = document.getElementById('voiceSampleStatus');
                if (status) {
                    status.textContent = `Sample loaded: ${char.voiceSamplePath}`;
                    status.classList.add('text-emerald-400');
                }
                const clearBtn = document.getElementById('clearVoiceSampleBtn');
                if (clearBtn) clearBtn.classList.remove('hidden');
            }

            colorPicker.querySelectorAll('button').forEach(btn => {
                btn.classList.remove('ring-2', 'ring-white/20', 'ring-offset-2', 'ring-offset-surface');
                if (btn.dataset.color === char.avatarColor) {
                    btn.classList.add('ring-2', 'ring-white/20', 'ring-offset-2', 'ring-offset-surface');
                }
            });
        }
    } else {
        title.textContent = 'Create New Chat';
        deleteBtn.classList.add('hidden');
    }

    modal.classList.remove('hidden');
    document.body.classList.add('modal-open');
    updateAvatarPreview(charId ? (APP_STATE.characters.find(c => c.id === charId)?.avatarUrl || '') : '');
    nameInput.focus();
    lucide.createIcons();
}

function closeCharacterEditor() {
    document.getElementById('characterEditorModal').classList.add('hidden');
    document.body.classList.remove('modal-open');
}

// Simple WAV encoder (16-bit PCM, mono)
function encodeWAV(audioBuffer) {
    const numChannels = 1; // force mono
    const sampleRate = audioBuffer.sampleRate;
    const samples = audioBuffer.getChannelData(0); // take first channel as mono
    const buffer = new ArrayBuffer(44 + samples.length * 2);
    const view = new DataView(buffer);

    // RIFF header
    writeString(view, 0, 'RIFF');
    view.setUint32(4, 36 + samples.length * 2, true);
    writeString(view, 8, 'WAVE');
    writeString(view, 12, 'fmt ');
    view.setUint32(16, 16, true);
    view.setUint16(20, 1, true); // PCM
    view.setUint16(22, numChannels, true);
    view.setUint32(24, sampleRate, true);
    view.setUint32(28, sampleRate * numChannels * 2, true);
    view.setUint16(32, numChannels * 2, true);
    view.setUint16(34, 16, true); // bits per sample
    writeString(view, 36, 'data');
    view.setUint32(40, samples.length * 2, true);

    // PCM data
    let offset = 44;
    for (let i = 0; i < samples.length; i++) {
        const s = Math.max(-1, Math.min(1, samples[i]));
        view.setInt16(offset, s < 0 ? s * 0x8000 : s * 0x7FFF, true);
        offset += 2;
    }

    return new Blob([buffer], { type: 'audio/wav' });
}

function writeString(view, offset, string) {
    for (let i = 0; i < string.length; i++) {
        view.setUint8(offset + i, string.charCodeAt(i));
    }
}

// Voice editor UI helper (for Qwen3-TTS VoiceDesign style models)
function setupVoiceEditorUI() {
    const modeButtons = document.querySelectorAll('#voiceModeSelector .voice-mode-btn');
    const presetSection = document.getElementById('voicePresetSection');
    const customSection = document.getElementById('voiceCustomSection');

    if (!modeButtons.length) return;

    modeButtons.forEach(btn => {
        btn.onclick = () => {
            modeButtons.forEach(b => b.classList.remove('border-accent', 'bg-accent/15', 'text-accent-light'));
            btn.classList.add('border-accent', 'bg-accent/15', 'text-accent-light');

            const mode = btn.dataset.voiceMode;
            if (presetSection) presetSection.classList.toggle('hidden', mode !== 'preset');
            if (customSection) customSection.classList.toggle('hidden', mode !== 'custom');
        };
    });

    // Default to "none"
    const noneBtn = document.querySelector('#voiceModeSelector [data-voice-mode="none"]');
    if (noneBtn) {
        modeButtons.forEach(b => b.classList.remove('border-accent', 'bg-accent/15', 'text-accent-light'));
        noneBtn.classList.add('border-accent', 'bg-accent/15', 'text-accent-light');
    }

    // Voice sample upload (basic wiring)
    const uploadBtn = document.getElementById('uploadVoiceSampleBtn');
    const clearBtn = document.getElementById('clearVoiceSampleBtn');
    const statusEl = document.getElementById('voiceSampleStatus');
    const fileInput = document.getElementById('voiceSampleFileInput');

    if (uploadBtn && fileInput && statusEl) {
        uploadBtn.onclick = () => fileInput.click();

        fileInput.onchange = async (e) => {
            const file = e.target.files[0];
            if (!file) return;

            statusEl.textContent = `Processing ${file.name}...`;
            statusEl.classList.add('text-emerald-400');
            if (clearBtn) clearBtn.classList.remove('hidden');

            try {
                // Decode any supported audio format using the browser
                const arrayBuffer = await file.arrayBuffer();
                const audioContext = new (window.AudioContext || window.webkitAudioContext)();
                const audioBuffer = await audioContext.decodeAudioData(arrayBuffer);

                // Convert to 16kHz mono (excellent for most voice models)
                const targetSampleRate = 16000;
                const offlineContext = new OfflineAudioContext(1, Math.ceil(audioBuffer.duration * targetSampleRate), targetSampleRate);
                const source = offlineContext.createBufferSource();
                source.buffer = audioBuffer;

                // Downmix to mono if needed
                if (audioBuffer.numberOfChannels > 1) {
                    const splitter = offlineContext.createChannelMerger(1);
                    const merger = offlineContext.createChannelMerger(1);
                    // Simple average downmix
                    const left = offlineContext.createGain();
                    const right = offlineContext.createGain();
                    // This is simplified - for production a proper downmix would be better
                }

                source.connect(offlineContext.destination);
                source.start(0);

                const renderedBuffer = await offlineContext.startRendering();

                // Encode to WAV (base64)
                const wavBlob = encodeWAV(renderedBuffer);
                const reader = new FileReader();
                reader.onload = () => {
                    const dataUrl = reader.result;
                    // Store temporarily — will be saved properly when user clicks "Start Chat"
                    window.__pendingVoiceSampleDataUrl = dataUrl;
                    window.__pendingVoiceSamplePath = null; // will be set after save
                    statusEl.textContent = `Sample ready: ${file.name} (will be saved with character)`;
                };
                reader.readAsDataURL(wavBlob);

            } catch (err) {
                console.error('Audio processing failed:', err);
                statusEl.textContent = `Failed to process ${file.name}. Try WAV or MP3.`;
                statusEl.classList.remove('text-emerald-400');
                statusEl.classList.add('text-red-400');
            }
        };
    }

    if (clearBtn && statusEl) {
        clearBtn.onclick = () => {
            statusEl.textContent = 'No sample uploaded';
            statusEl.classList.remove('text-emerald-400');
            clearBtn.classList.add('hidden');
            if (fileInput) fileInput.value = '';
        };
    }

    // === Knowledge Base (RAG) Upload ===
    const uploadKnowledgeBtn = document.getElementById('uploadKnowledgeBtn');
    const knowledgeStatus = document.getElementById('knowledgeStatus');
    const knowledgeFileInput = document.getElementById('knowledgeFileInput');
    const knowledgeList = document.getElementById('knowledgeList');

    if (uploadKnowledgeBtn && knowledgeFileInput) {
        uploadKnowledgeBtn.onclick = () => knowledgeFileInput.click();

        knowledgeFileInput.onchange = async (e) => {
            const files = Array.from(e.target.files || []);
            if (files.length === 0) return;

            const editingId = document.getElementById('saveCharBtn').dataset.editingId;
            // Must use final character id — temp-* paths never remapped → empty RAG.
            if (!editingId) {
                knowledgeStatus.textContent = 'Save the contact first, then attach knowledge documents.';
                showToast('Save the contact first, then upload knowledge documents.', 'warning');
                e.target.value = '';
                return;
            }

            knowledgeStatus.textContent = `Uploading ${files.length} document(s)...`;

            for (const file of files) {
                try {
                    const arrayBuffer = await file.arrayBuffer();
                    const bytes = new Uint8Array(arrayBuffer);

                    const savedPath = await callTauri('save_character_document', {
                        characterId: editingId,
                        filename: file.name,
                        data: Array.from(bytes),
                    });

                    if (savedPath) {
                        // Track in a temp array for saving with character
                        if (!window.__pendingKnowledgeDocs) window.__pendingKnowledgeDocs = [];
                        window.__pendingKnowledgeDocs.push({ name: file.name, path: savedPath });
                    }
                } catch (err) {
                    console.error('Failed to upload document', file.name, err);
                }
            }

            refreshKnowledgeListUI(knowledgeList, knowledgeStatus);
        };
    }
}

function refreshKnowledgeListUI(listEl, statusEl) {
    if (!listEl) return;

    const docs = window.__pendingKnowledgeDocs || [];
    if (docs.length === 0) {
        listEl.innerHTML = '';
        if (statusEl) statusEl.textContent = 'No documents attached';
        return;
    }

    listEl.innerHTML = docs.map((doc, idx) => `
        <div class="flex items-center justify-between bg-surface-light/50 px-2 py-1 rounded text-xs">
            <span class="truncate">${escapeHtml(doc.name)}</span>
            <button class="text-red-400 hover:text-red-300 px-1" data-idx="${idx}">✕</button>
        </div>
    `).join('');

    // Wire remove buttons
    listEl.querySelectorAll('button[data-idx]').forEach(btn => {
        btn.onclick = () => {
            const idx = parseInt(btn.dataset.idx);
            if (window.__pendingKnowledgeDocs) {
                window.__pendingKnowledgeDocs.splice(idx, 1);
            }
            refreshKnowledgeListUI(listEl, statusEl);
        };
    });

    if (statusEl) statusEl.textContent = `${docs.length} document(s) attached`;
}

async function saveCharacter() {
    const editingId = document.getElementById('saveCharBtn').dataset.editingId;
    const name = document.getElementById('charNameInput').value.trim();
    let avatarValue = document.getElementById('charAvatarUrlInput').value.trim();
    const personality = document.getElementById('charPersonalityInput').value.trim();
    const userDescription = document.getElementById('charUserDescInput').value.trim();
    const scenario = document.getElementById('charScenarioInput').value.trim();
    const writingInstructions = document.getElementById('charWritingInstructionsInput').value.trim();
    const systemPrompt = document.getElementById('charSystemPromptInput').value.trim();
    const greeting = document.getElementById('charGreetingInput').value.trim();
    const userNickname = document.getElementById('charUserNicknameInput').value.trim();

    // Voice settings (new)
    const activeVoiceModeBtn = document.querySelector('#voiceModeSelector .voice-mode-btn.border-accent');
    const voiceMode = activeVoiceModeBtn ? activeVoiceModeBtn.dataset.voiceMode : 'none';
    const voicePreset = document.getElementById('charVoicePresetInput')?.value || null;
    let voiceSamplePath = window.__pendingVoiceSamplePath || null; // captured during upload

    // If we have a fresh large audio sample waiting, save it now
    if (window.__pendingVoiceSampleDataUrl && window.__pendingVoiceSampleDataUrl.length > 1000) {
        const tempId = editingId || `temp-${Date.now()}`;
        const savedPath = await callTauri('save_voice_sample', {
            characterId: tempId,
            dataUrl: window.__pendingVoiceSampleDataUrl
        });
        if (savedPath) {
            voiceSamplePath = savedPath;
        }
        // Clean up
        delete window.__pendingVoiceSampleDataUrl;
        delete window.__pendingVoiceSamplePath;
    }

    const selectedColorBtn = document.querySelector('#colorPicker button.ring-2');
    const avatarColor = selectedColorBtn ? selectedColorBtn.dataset.color : '#7c3aed';

    const activeModeBtn = document.querySelector('#modeSelector .mode-card.border-accent');
    const mode = activeModeBtn ? activeModeBtn.dataset.mode : 'advanced-chat';

    if (!name) {
        showToast('Please enter a bot/NPC name.', 'warning');
        return;
    }

    // === NEW: If this is a fresh uploaded image (large data URL), persist it properly via Rust ===
    if (avatarValue.startsWith('data:image') && avatarValue.length > 10000) {
        // This is a new upload — save it to disk via backend
        const tempId = editingId || `temp-${Date.now()}`;
        const savedPath = await callTauri('save_character_avatar', {
            characterId: tempId,
            dataUrl: avatarValue
        });

        if (savedPath) {
            avatarValue = savedPath; // store only the relative path now
        } else {
            console.warn('[LocalPersona] Failed to save avatar via backend, keeping data URL temporarily');
        }
    }

    const charData = {
        id: editingId || `custom-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
        name,
        mode,
        avatarColor,
        avatarUrl: avatarValue, // can now be a path like "avatars/xxx.png" or a data URL / external URL
        personality: personality || 'A friendly AI companion.',
        userNickname,
        userDescription,
        scenario,
        writingInstructions,
        systemPrompt,
        greeting: greeting || `Hello! I'm ${name}. How can I help you today?`,
        createdAt: editingId ? (APP_STATE.characters.find(c => c.id === editingId)?.createdAt || Date.now()) : Date.now(),

        // Voice (new - for local TTS models like Qwen3-TTS VoiceDesign)
        voiceMode: voiceMode || 'none',
        voicePreset: voicePreset || null,
        voiceSamplePath: voiceSamplePath || null,

        // Future AR/Social fields (for Meta + Pokémon GO vision)
        spawnLocation: null,     // { lat, lng, radius? }
        isPublicSpawn: false,

        // Knowledge Base (RAG) — preserve existing flag; only set true when new docs pending
        has_knowledge_base:
            !!(window.__pendingKnowledgeDocs && window.__pendingKnowledgeDocs.length > 0)
            || !!(editingId && APP_STATE.characters.find(c => c.id === editingId)?.has_knowledge_base)
            || !!(editingId && APP_STATE.characters.find(c => c.id === editingId)?.hasKnowledgeBase)
            || false,
    };

    if (editingId) {
        const index = APP_STATE.characters.findIndex(c => c.id === editingId);
        if (index !== -1) APP_STATE.characters[index] = charData;
    } else {
        APP_STATE.characters.push(charData);
    }

    // Persist the changed character to disk (granular) — primary method
    const saved = await saveCharacterToDisk(charData);
    if (!saved) {
        showToast('Failed to save contact to disk. Changes are not persisted.', 'error');
        // Roll back in-memory if this was a new contact
        if (!editingId) {
            APP_STATE.characters = APP_STATE.characters.filter(c => c.id !== charData.id);
        }
        return;
    }

    window.__pendingKnowledgeDocs = [];
    renderCharacterList();
    renderQuickSwitchCards();

    if (!APP_STATE.activeCharacterId) {
        await selectCharacter(charData.id);
    } else if (APP_STATE.activeCharacterId === charData.id) {
        await updateChatView();
    } else {
        await updateChatView();
    }

    closeCharacterEditor();
    showToast('Contact saved.', 'success');
}

async function deleteCharacter(charId) {
    const char = APP_STATE.characters.find(c => c.id === charId);
    if (!char) return;

    const confirmModal = document.getElementById('confirmDeleteModal');
    confirmModal.classList.remove('hidden');
    document.body.classList.add('modal-open');

    const handleConfirm = async () => {
        APP_STATE.characters = APP_STATE.characters.filter(c => c.id !== charId);
        await deleteCharacterFromDisk(charId);

        // Characters are only on disk now.
        // Clean up any old per-character legacy chat history keys from previous versions.
        try {
            localStorage.removeItem(`localpersona_${getChatHistoryKey(charId)}`);
        } catch (_) {}

        if (APP_STATE.activeCharacterId === charId) {
            APP_STATE.activeCharacterId = APP_STATE.characters.length > 0 ? APP_STATE.characters[0].id : null;
            saveToStorage('activeCharacterId', APP_STATE.activeCharacterId);
            currentConversationId = null;
            renderedMessageIds = new Set();
            if (APP_STATE.activeCharacterId) {
                await selectCharacter(APP_STATE.activeCharacterId);
                closeCharacterEditor();
                closeConfirmDelete();
                return;
            }
        }

        renderCharacterList();
        renderQuickSwitchCards();
        await updateChatView();
        closeCharacterEditor();
        closeConfirmDelete();
    };

    document.getElementById('confirmDeleteBtn').onclick = handleConfirm;
    document.getElementById('cancelDeleteBtn').onclick = closeConfirmDelete;
}

function closeConfirmDelete() {
    document.getElementById('confirmDeleteModal').classList.add('hidden');
    document.body.classList.remove('modal-open');
}

// --- CHAT + LLM CALLS ---
// Phase A: Single source of truth = append-only conversations/{uuid}/.

/**
 * Active conversation messages (NDJSON path only).
 * Ensures a conversation exists for the active character when needed.
 */
async function getActiveChatHistory() {
    if (!APP_STATE.activeCharacterId) return [];
    if (!currentConversationId) {
        const activeChar = APP_STATE.characters.find(c => c.id === APP_STATE.activeCharacterId);
        await getOrCreateConversationForCharacter(
            APP_STATE.activeCharacterId,
            activeChar?.name || 'Contact'
        );
    }
    if (!currentConversationId) return [];
    // Full load for action checks (regenerate / copy); UI rendering uses bounded load.
    return await loadConversationMessages(false);
}

async function saveActiveChatHistory(_history) {
    console.warn('[LocalPersona] saveActiveChatHistory is a no-op; messages are appended by the Rust pipeline.');
}

// ===================================================================
// NEW CONVERSATION SYSTEM HELPERS (Phase 1 Hard Cutover)
// These replace the old per-character chat history system.
// ===================================================================

/**
 * Gets an existing conversation for a character, or creates a new one.
 * Sets currentConversationId as a side effect.
 */
async function getOrCreateConversationForCharacter(characterId, characterName) {
    try {
        // Try to find an existing conversation for this character
        const previews = await callTauri('list_conversation_previews');
        
        if (previews && Array.isArray(previews)) {
            const existing = previews.find(p => 
                p.participant_ids && p.participant_ids.includes(characterId)
            );
            if (existing) {
                currentConversationId = existing.id;
                return existing.id;
            }
        }

        // No existing conversation — create one
        const newConv = await callTauri('create_conversation_from_character', {
            characterId: characterId,
            characterName: characterName,
        });

        if (newConv && newConv.id) {
            currentConversationId = newConv.id;
            return newConv.id;
        }
    } catch (e) {
        console.error('Failed to get/create conversation:', e);
    }

    return null;
}

/**
 * Loads messages for the current conversation using the append-only system.
 */
async function loadConversationMessages(useBounded = true) {
    if (!currentConversationId) return [];
    try {
        if (useBounded) {
            // Phase 2: Use token-budget bounded load for performance on long conversations.
            // This directly addresses the audit finding that UI was loading entire NDJSON
            // even though inference was already protected.
            const result = await callTauri('load_messages_for_display', {
                conversationId: currentConversationId,
                maxTokens: 16384, // Generous UI budget (separate from inference 8k)
            });
            // Store has_more for the "Load earlier" button
            window.currentConversationHasMore = result?.has_more ?? false;
            return result?.messages || [];
        } else {
            // Explicit full load (used for export, repair, or "load everything" power action)
            return await callTauri('load_all_messages', {
                conversationId: currentConversationId,
            }) || [];
        }
    } catch (e) {
        console.error('Failed to load conversation messages:', e);
        return [];
    }
}

/**
 * Phase 2: Load more older messages (increases effective context for the UI).
 * Currently does a larger bounded load. Future improvement: true cursor-based older window.
 */
async function loadMoreOlderMessages() {
    if (!currentConversationId || isLoadingMoreMessages) return;

    const container = document.getElementById('messagesContainer');
    if (!container) return;

    isLoadingMoreMessages = true;
    const btn = document.getElementById('loadEarlierBtn');
    if (btn) btn.disabled = true;

    try {
        // Request a significantly larger window (roughly double the normal UI budget)
        const result = await callTauri('load_messages_for_display', {
            conversationId: currentConversationId,
            maxTokens: 32768,
        });

        const olderMessages = result?.messages || [];
        window.currentConversationHasMore = result?.has_more ?? false;

        if (olderMessages.length === 0) {
            updateLoadEarlierButton(false);
            return;
        }

        const activeChar = APP_STATE.characters.find(c => c.id === APP_STATE.activeCharacterId);
        if (!activeChar) return;

        // Prepend older messages (avoiding duplicates via renderedMessageIds)
        const fragment = document.createDocumentFragment();
        let prependedCount = 0;

        for (const msg of olderMessages) {
            if (renderedMessageIds.has(msg.id)) continue;

            const html = renderSingleMessage(msg, activeChar);
            const temp = document.createElement('div');
            temp.innerHTML = html;
            const el = temp.firstElementChild;

            if (el) {
                fragment.appendChild(el);
                renderedMessageIds.add(msg.id);
                prependedCount++;
            }
        }

        if (prependedCount > 0) {
            // Insert at the top while preserving scroll position
            const scrollTopBefore = container.scrollTop;
            container.insertBefore(fragment, container.firstChild);
            container.scrollTop = scrollTopBefore + 20; // small adjustment

            // Re-attach action listeners for the newly prepended messages
            attachMessageActionListenersToNewElements(container);
        }

        updateLoadEarlierButton(window.currentConversationHasMore);

    } catch (e) {
        console.error('Failed to load more older messages:', e);
        showToast('Failed to load earlier messages', 'error');
    } finally {
        isLoadingMoreMessages = false;
        if (btn) btn.disabled = false;
    }
}

function updateLoadEarlierButton(show) {
    let btn = document.getElementById('loadEarlierBtn');
    const container = document.getElementById('messagesContainer');

    if (!container) return;

    if (show) {
        if (!btn) {
            btn = document.createElement('button');
            btn.id = 'loadEarlierBtn';
            btn.className = 'mx-auto my-3 px-4 py-1.5 text-xs rounded-full bg-surface-light hover:bg-surface border border-gray-700 text-gray-400 hover:text-white transition-colors flex items-center gap-2';
            btn.innerHTML = `<i data-lucide="chevron-up" class="w-3.5 h-3.5"></i> Load earlier messages`;
            btn.onclick = () => loadMoreOlderMessages();

            // Insert as first child of the messages area or before the messages container
            const parent = container.parentElement;
            if (parent) {
                parent.insertBefore(btn, container);
            } else {
                container.parentNode?.insertBefore(btn, container);
            }
            if (window.lucide) window.lucide.createIcons();
        }
        btn.style.display = 'flex';
    } else if (btn) {
        btn.style.display = 'none';
    }
}

function attachMessageActionListenersToNewElements(container) {
    // Re-attach for any newly inserted elements (used after prepending older messages)
    container.querySelectorAll('.message-group:not([data-listeners-attached])').forEach(el => {
        el.setAttribute('data-listeners-attached', 'true');
        attachMessageActionListeners(el);
    });
}

async function sendMessage() {
    if (APP_STATE.isGenerating || !APP_STATE.activeCharacterId) return;

    const input = document.getElementById('chatInput');
    const content = input.value.trim();
    const hasImages = pendingImages.length > 0;

    if (!content && !hasImages) return;

    const activeChar = APP_STATE.characters.find(c => c.id === APP_STATE.activeCharacterId);
    if (!activeChar) return;

    // R0 identity contract: ensure a real conversation UUID exists before send.
    // Never use character id as conversation_id (causes split-brain history).
    if (!currentConversationId) {
        const convId = await getOrCreateConversationForCharacter(activeChar.id, activeChar.name);
        if (!convId) {
            showToast('Could not open conversation for this contact.', 'error');
            return;
        }
    }

    const sendBtn = document.getElementById('sendBtn');
    if (sendBtn) sendBtn.disabled = true;
    APP_STATE.isGenerating = true;
    const genToken = ++_inflightGenerationToken;
    const sendConversationId = currentConversationId;
    const sendCharacterId = activeChar.id;
    let sendSucceeded = false;

    const s = APP_STATE.settings || {};
    // Tauri 2: invoke args must be camelCase (maps to Rust snake_case params).
    const payload = {
        conversationId: sendConversationId,
        characterId: sendCharacterId,
        speakerId: 'user',
        speakerName: activeChar.userNickname || 'You',
        textContent: content,
        imagePaths: hasImages ? pendingImages.map(i => i.path) : [],
        temperature: typeof s.temperature === 'number' ? s.temperature : 0.7,
        maxTokens: typeof s.maxTokens === 'number' ? s.maxTokens : 2048,
        topP: typeof s.topP === 'number' ? s.topP : 0.9,
    };

    try {
        // Unified path (vision + text): Rust owns compose, RAG, persistence, Arena.
        showStreamingMessage(); // loading chrome (non-stream completion)
        await callTauri('send_message_with_images', payload);
        sendSucceeded = true;
        // Clear composer only after success so failed sends keep user text/images.
        input.value = '';
        input.style.height = 'auto';
        const hint = document.getElementById('charCountHint');
        if (hint) hint.classList.add('hidden');
        if (hasImages) clearPendingImages();
        // Only refresh UI if user is still on the same contact/conversation.
        if (
            genToken === _inflightGenerationToken
            && APP_STATE.activeCharacterId === sendCharacterId
            && currentConversationId === sendConversationId
        ) {
            await updateChatView();
            updateContextUsagePill();
        }
    } catch (err) {
        console.error('Message send failed (unified path):', err);
        showToast(`Error sending message: ${err}`, 'error');
    } finally {
        hideStreamingMessage();
        if (sendBtn) sendBtn.disabled = false;
        APP_STATE.isGenerating = false;
        if (!sendSucceeded && APP_STATE.activeCharacterId === sendCharacterId) {
            updateChatView().catch(() => {});
        }
        updateLoadEarlierButton(window.currentConversationHasMore);
    }
}

async function regenerateLastResponse() {
    if (APP_STATE.isGenerating || !APP_STATE.activeCharacterId) return;

    // Phase A: only NDJSON conversation path
    if (!currentConversationId) {
        const activeChar = APP_STATE.characters.find(c => c.id === APP_STATE.activeCharacterId);
        const convId = await getOrCreateConversationForCharacter(
            APP_STATE.activeCharacterId,
            activeChar?.name || 'Contact'
        );
        if (!convId) {
            showToast('No conversation to regenerate.', 'error');
            return;
        }
    }

    const history = await loadConversationMessages(false);
    if (history.length === 0) {
        showToast('Nothing to regenerate yet.', 'info');
        return;
    }
    // Optional client-side guard (backend also validates last user turn)
    const last = history[history.length - 1];
    const hasUser = history.some(m => m.role === 'user');
    if (!hasUser) {
        showToast('No user message to regenerate from.', 'info');
        return;
    }
    if (last && last.role !== 'assistant' && last.role !== 'user') {
        await updateChatView();
        return;
    }

    document.getElementById('sendBtn').disabled = true;
    document.getElementById('regenerateBtn').classList.add('hidden');
    showStreamingMessage();
    APP_STATE.isGenerating = true;

    try {
        const s = APP_STATE.settings || {};
        await callTauri('regenerate_last_message', {
            conversationId: currentConversationId,
            characterId: APP_STATE.activeCharacterId,
            temperature: typeof s.temperature === 'number' ? s.temperature : 0.7,
            maxTokens: typeof s.maxTokens === 'number' ? s.maxTokens : 2048,
            topP: typeof s.topP === 'number' ? s.topP : 0.9,
        });
        await updateChatView();
        updateContextUsagePill();
    } catch (err) {
        console.error('Regeneration failed:', err);
        showToast(`Regeneration failed: ${err}`, 'error');
    } finally {
        hideStreamingMessage();
        const sendBtn = document.getElementById('sendBtn');
        if (sendBtn) sendBtn.disabled = false;
        APP_STATE.isGenerating = false;
        // Restore regen visibility even on failure
        try { await updateChatView(); } catch (_) {}
    }
}

async function regenerateFrom(messageIndex) {
    if (APP_STATE.isGenerating || !APP_STATE.activeCharacterId || !currentConversationId) return;

    const history = await loadConversationMessages(false);
    if (!history.length) {
        showToast('Nothing to regenerate yet.', 'info');
        return;
    }

    const idx = Number(messageIndex);
    if (!Number.isFinite(idx) || idx < 0 || idx >= history.length) {
        return regenerateLastResponse();
    }

    // Walk back to the nearest user turn at or before the clicked message.
    let target = null;
    for (let i = idx; i >= 0; i--) {
        if (history[i].role === 'user') {
            target = history[i];
            break;
        }
    }
    if (!target || !target.id) {
        showToast('No user message at or before this turn.', 'info');
        return;
    }

    // Mid-thread regenerate drops all later turns (backend keeps a .bak snapshot).
    const targetIdx = history.findIndex((m) => m.id === target.id);
    if (targetIdx >= 0 && targetIdx < history.length - 1) {
        const ok = window.confirm(
            'Regenerate from this message will remove all later turns in this chat (a backup is saved on disk). Continue?'
        );
        if (!ok) return;
    }

    document.getElementById('sendBtn').disabled = true;
    document.getElementById('regenerateBtn')?.classList.add('hidden');
    showStreamingMessage();
    APP_STATE.isGenerating = true;

    try {
        const s = APP_STATE.settings || {};
        await callTauri('regenerate_last_message', {
            conversationId: currentConversationId,
            characterId: APP_STATE.activeCharacterId,
            temperature: typeof s.temperature === 'number' ? s.temperature : 0.7,
            maxTokens: typeof s.maxTokens === 'number' ? s.maxTokens : 2048,
            topP: typeof s.topP === 'number' ? s.topP : 0.9,
            fromMessageId: target.id,
        });
        await updateChatView();
        updateContextUsagePill();
    } catch (err) {
        console.error('Regenerate-from failed:', err);
        showToast(`Regeneration failed: ${err}`, 'error');
    } finally {
        hideStreamingMessage();
        const sendBtn = document.getElementById('sendBtn');
        if (sendBtn) sendBtn.disabled = false;
        APP_STATE.isGenerating = false;
        try { await updateChatView(); } catch (_) {}
    }
}

async function editMessage(messageIndex) {
    // R3: Edit deferred for RC — UI entry points hidden; toast if reached via legacy path.
    showToast('Message editing is not available in this RC. Copy the message and resend instead.', 'info');
}

async function editMessageById(messageId) {
    showToast('Message editing is not available in this RC.', 'info');
}

/** Context usage estimate: message tokens vs model context (from inference status) or UI budget. */
async function updateContextUsagePill() {
    const pill = document.getElementById('contextUsagePill');
    if (!pill || !currentConversationId) {
        if (pill) pill.classList.add('hidden');
        return;
    }
    try {
        const [result, status] = await Promise.all([
            callTauri('load_messages_for_display', {
                conversationId: currentConversationId,
                maxTokens: 16384,
            }),
            callTauri('get_inference_status').catch(() => null),
        ]);
        const msgs = result?.messages || [];
        let tokens = 0;
        for (const m of msgs) {
            tokens += m.token_count || Math.ceil((m.content || '').length / 4);
        }
        const modelCtx = status?.model_context_length || status?.modelContextLength || null;
        // Prefer real model context; fall back to UI window budget.
        const budget = modelCtx || result?.budget_used || 16384;
        const pct = Math.min(100, Math.round((tokens / budget) * 100));
        const label = modelCtx
            ? `Context ~${tokens.toLocaleString()} / ${Number(modelCtx).toLocaleString()} (${pct}%)`
            : `Context ~${tokens.toLocaleString()} tok (${pct}%)`;
        pill.textContent = label;
        pill.title =
            `${msgs.length} messages loaded · ` +
            (modelCtx
                ? `model context ${Number(modelCtx).toLocaleString()} tokens`
                : `UI budget ${Number(result?.budget_used || 16384).toLocaleString()} tokens`) +
            (result?.has_more ? ' · earlier messages available' : '');
        pill.classList.remove('hidden');
    } catch (_) {
        pill.classList.add('hidden');
    }
}

async function copyMessage(messageIndex) {
    // Phase 1 transition: prefer new conversation system when available
    const messages = currentConversationId 
        ? await loadConversationMessages() 
        : await getActiveChatHistory();

    const msg = messages[messageIndex];
    if (!msg) return;

    navigator.clipboard.writeText(msg.content).then(() => {
        const btn = document.querySelector(`[data-action="copy-message"][data-index="${messageIndex}"]`);
        if (btn) {
            const icon = btn.querySelector('i');
            if (icon) {
                icon.setAttribute('data-lucide', 'check');
                lucide.createIcons();
                setTimeout(() => {
                    icon.setAttribute('data-lucide', 'copy');
                    lucide.createIcons();
                }, 1500);
            }
        }
    }).catch(err => console.warn('Failed to copy:', err));
}

function showStreamingMessage() {
    const streamingEl = document.getElementById('streamingMessage');
    streamingEl.classList.remove('hidden');
    APP_STATE.streamingContent = '';
    scrollToBottom();
}

function hideStreamingMessage() {
    const streamingEl = document.getElementById('streamingMessage');
    streamingEl.classList.add('hidden');
    APP_STATE.streamingContent = '';
}

// Messenger-style typing indicator
function showTypingIndicator(contactName) {
    const indicator = document.getElementById('typingIndicator');
    const nameEl = document.getElementById('typingContactName');
    if (!indicator) return;

    if (nameEl) nameEl.textContent = contactName || 'Contact';
    indicator.classList.remove('hidden');
    scrollToBottom();
}

function hideTypingIndicator() {
    const indicator = document.getElementById('typingIndicator');
    if (indicator) indicator.classList.add('hidden');
}

function updateStreamingContent(content) {
    APP_STATE.streamingContent = content;
    const streamingEl = document.getElementById('streamingMessage');
    if (streamingEl.classList.contains('hidden')) return;

    const bubble = streamingEl.querySelector('.bg-chat-ai');
    if (bubble) {
        const dots = bubble.querySelector('.streaming-dots');
        if (dots) dots.remove();
        let contentEl = bubble.querySelector('.streaming-text');
        if (!contentEl) {
            const p = document.createElement('p');
            p.className = 'text-sm text-gray-200 message-content whitespace-pre-wrap streaming-text';
            p.textContent = content;
            bubble.appendChild(p);
        } else {
            contentEl.textContent = content;
        }
    }
    scrollToBottom();
}

// --- LLM CALL (original logic kept for compatibility) ---
// Phase 12.0: callLocalLLM removed — all LLM calls now go through the unified Rust pipeline
// (send_message_with_images / regenerate_last_message commands).
// The legacy direct-fetch path was a security and consistency risk:
// - No Arena Reset counter tracking
// - No RAG retrieval
// - No character-aware system prompt from Rust
// - No atomic persistence guarantee
// - Bypassed all input validation

// --- CONNECTION & SETTINGS (kept as-is) ---
async function testConnection(showResult = true) {
    const settings = APP_STATE.settings;
    let endpoint = settings.apiEndpoint;
    let testUrl;

    if (settings.apiType === 'ollama') {
        const base = endpoint.replace(/\/api\/chat\/?$/, '').replace(/\/v1\/chat\/completions\/?$/, '').replace(/\/$/, '');
        testUrl = base + '/api/tags';
    } else {
        const base = endpoint.replace(/\/v1\/chat\/completions\/?$/, '').replace(/\/$/, '');
        testUrl = base + '/v1/models';
    }

    try {
        const controller = new AbortController();
        const timeout = setTimeout(() => controller.abort(), 5000);

        const response = await fetch(testUrl, {
            method: 'GET',
            signal: controller.signal,
            headers: { 'Content-Type': 'application/json' },
        });
        clearTimeout(timeout);

        if (response.ok) {
            updateConnectionStatus(true);
            if (showResult) {
                const resultEl = document.getElementById('connectionTestResult');
                resultEl.textContent = '✅ Connection successful! Server is reachable.';
                resultEl.className = 'text-xs text-center text-green-400';
                resultEl.classList.remove('hidden');
                setTimeout(() => resultEl.classList.add('hidden'), 4000);
            }
            return true;
        } else {
            throw new Error(`Status ${response.status}`);
        }
    } catch (error) {
        updateConnectionStatus(false);
        if (showResult) {
            const resultEl = document.getElementById('connectionTestResult');
            resultEl.textContent = `❌ Connection failed: ${error.message}`;
            resultEl.className = 'text-xs text-center text-red-400';
            resultEl.classList.remove('hidden');
            setTimeout(() => resultEl.classList.add('hidden'), 6000);
        }
        return false;
    }
}

function updateConnectionStatus(connected) {
    APP_STATE.isConnected = connected;
    const dot = document.getElementById('connectionDot');
    const banner = document.getElementById('connectionBanner');

    if (connected) {
        dot.className = 'w-2.5 h-2.5 rounded-full bg-green-400 shadow-lg shadow-green-500/40 transition-all duration-500';
        dot.title = 'Connected to LLM server';
        banner.classList.add('hidden');
    } else {
        dot.className = 'w-2.5 h-2.5 rounded-full bg-red-400 shadow-lg shadow-red-500/40 animate-pulse transition-all duration-500';
        dot.title = 'Not connected - check server';
        banner.classList.remove('hidden');
    }
}

function openSettings() {
    const modal = document.getElementById('settingsModal');
    if (!modal) {
        console.error('[LocalPersona] settingsModal element missing from DOM');
        if (typeof showToast === 'function') {
            showToast('Settings UI failed to load (modal missing).', 'error');
        }
        return;
    }

    // Always show the modal first — never block open on form-fill errors.
    // Inline display is a hard fallback if Tailwind CDN/.hidden CSS fails to load.
    modal.classList.remove('hidden');
    modal.style.display = 'flex';
    document.body.classList.add('modal-open');

    try {
        applySettingsToUI();
    } catch (e) {
        console.error('[LocalPersona] applySettingsToUI failed:', e);
        if (typeof showToast === 'function') {
            showToast('Settings opened with defaults (form fill error).', 'warning');
        }
    }

    // Refresh inference server status when opening settings
    setTimeout(() => {
        try {
            if (typeof window.refreshServerStatusUI === 'function') {
                window.refreshServerStatusUI();
            }
        } catch (e) {
            console.warn('[LocalPersona] refreshServerStatusUI failed:', e);
        }
    }, 120);

    // Auto-scan for models when opening Settings
    setTimeout(() => {
        try {
            if (typeof window.refreshDiscoveredModels === 'function') {
                window.refreshDiscoveredModels();
            }
        } catch (e) {
            console.warn('[LocalPersona] refreshDiscoveredModels failed:', e);
        }
    }, 280);

    // Restore persisted llama binary path in display
    setTimeout(async () => {
        try {
            const saved = await callTauri('get_llama_server_path');
            const disp = document.getElementById('llamaServerPathDisplay');
            if (saved && disp) {
                disp.textContent = saved;
                disp.classList.add('text-emerald-400');
            }
        } catch (_) {}
    }, 150);
}

function closeSettings() {
    const modal = document.getElementById('settingsModal');
    if (modal) {
        modal.classList.add('hidden');
        modal.style.display = '';
    }
    document.body.classList.remove('modal-open');
}

function applySettingsToUI() {
    const s = APP_STATE.settings || {};
    const setVal = (id, value) => {
        const el = document.getElementById(id);
        if (!el || value === undefined || value === null) return;
        if ('value' in el) el.value = value;
        else el.textContent = value;
    };
    const setText = (id, value) => {
        const el = document.getElementById(id);
        if (el && value !== undefined && value !== null) el.textContent = value;
    };

    setVal('apiTypeSelect', s.apiType ?? 'openai-compatible');
    setVal('apiEndpointInput', s.apiEndpoint ?? '');
    setVal('modelNameInput', s.model ?? '');
    setVal('temperatureSlider', s.temperature ?? 0.7);
    setText('tempValue', s.temperature ?? 0.7);
    setVal('maxTokensInput', s.maxTokens ?? 2048);
    setVal('topPSlider', s.topP ?? 0.9);
    setText('topPValue', s.topP ?? 0.9);

    // Load TTS settings if present
    const ttsEndpointInput = document.getElementById('ttsEndpointInput');
    if (ttsEndpointInput && s.ttsEndpoint) {
        ttsEndpointInput.value = s.ttsEndpoint;
    }
    const ttsModelInput = document.getElementById('ttsModelPathInput');
    if (ttsModelInput && s.ttsModelPath) {
        ttsModelInput.value = s.ttsModelPath;
    }

    // R2-A: stream toggle is frozen OFF for RC-stable
    if (APP_STATE.settings) APP_STATE.settings.stream = false;
    const toggle = document.getElementById('streamToggle');
    if (toggle) {
        const toggleKnob = toggle.querySelector('span');
        toggle.disabled = true;
        toggle.classList.remove('bg-accent');
        toggle.classList.add('bg-gray-600');
        if (toggleKnob) {
            toggleKnob.classList.remove('translate-x-5');
            toggleKnob.classList.add('translate-x-0');
        }
        toggle.setAttribute('aria-checked', 'false');
        toggle.setAttribute('aria-disabled', 'true');
    }
}

function saveSettings() {
    APP_STATE.settings.apiType = document.getElementById('apiTypeSelect').value;
    APP_STATE.settings.apiEndpoint = document.getElementById('apiEndpointInput').value.trim();
    APP_STATE.settings.model = document.getElementById('modelNameInput').value.trim();
    APP_STATE.settings.temperature = parseFloat(document.getElementById('temperatureSlider').value);
    APP_STATE.settings.maxTokens = parseInt(document.getElementById('maxTokensInput').value) || 2048;
    APP_STATE.settings.topP = parseFloat(document.getElementById('topPSlider').value);
    // R2-A: never persist stream=true in this RC
    APP_STATE.settings.stream = false;

    // Voice / TTS settings
    const ttsEndpoint = document.getElementById('ttsEndpointInput');
    if (ttsEndpoint) {
        APP_STATE.settings.ttsEndpoint = ttsEndpoint.value.trim();
    }
    const ttsModel = document.getElementById('ttsModelPathInput');
    if (ttsModel) {
        APP_STATE.settings.ttsModelPath = ttsModel.value.trim();
    }

    saveToStorage('settings', APP_STATE.settings);
    closeSettings();
    updateConnectionStatus(false);
    testConnection(true);
}

// === 10/10 Plan: Diagnostics Panel ===
function openDiagnostics() {
    const modal = document.getElementById('diagnosticsModal');
    const content = document.getElementById('diagnosticsContent');
    modal.classList.remove('hidden');

    content.innerHTML = '<div class="text-gray-400">Loading diagnostics snapshot...</div>';

    (async () => {
        try {
            const snapshot = await callTauri('get_diagnostics_snapshot').catch(() => ({}));

            let html = '';

            // LLM Server Card
            const llm = snapshot.llm_server || {};
            html += `
                <div class="bg-surface-light rounded-xl p-4 border border-gray-700/40">
                    <div class="font-semibold mb-2 flex items-center gap-2">
                        <i data-lucide="cpu" class="w-4 h-4"></i>
                        Main LLM Server
                    </div>
                    <div class="text-sm space-y-0.5">
                        <div><span class="text-gray-400">State:</span> <span class="font-mono">${llm.state || 'Unknown'}</span></div>
                        ${llm.port ? `<div><span class="text-gray-400">Port:</span> ${llm.port}</div>` : ''}
                        ${llm.model ? `<div class="truncate text-[10px]"><span class="text-gray-400">Model:</span> ${escapeHtml(llm.model.split('/').pop() || '')}</div>` : ''}
                        ${llm.arena ? `
                            <div class="pt-1 text-[11px] text-gray-400">
                                Arena: ${llm.arena.requests_since_reset || 0} / ${llm.arena.max_requests_before_reset || '?'} requests
                                ${llm.arena.uptime_seconds ? ` • Uptime: ${Math.floor(llm.arena.uptime_seconds / 60)}m` : ''}
                            </div>
                            ${llm.arena.last_reset_reason ? `
                                <div class="text-[10px] text-amber-400/90 mt-0.5">
                                    Last reset: ${escapeHtml(llm.arena.last_reset_reason)}
                                    ${llm.arena.last_reset_at_unix_ms ? ` (${new Date(llm.arena.last_reset_at_unix_ms).toLocaleString()})` : ''}
                                </div>
                            ` : ''}
                        ` : ''}
                        ${llm.model_context_length ? `
                            <div class="text-[10px] text-emerald-400 mt-0.5">Ctx: ${llm.model_context_length.toLocaleString()} tokens (dynamic)</div>
                        ` : ''}
                    </div>
                </div>
            `;

            // Voice Server Card
            const voice = snapshot.voice_server || {};
            html += `
                <div class="bg-surface-light rounded-xl p-4 border border-gray-700/40">
                    <div class="font-semibold mb-2 flex items-center gap-2">
                        <i data-lucide="volume-2" class="w-4 h-4"></i>
                        Voice / TTS Server <span class="text-[10px] text-amber-500/90 font-normal">Experimental</span>
                    </div>
                    <div class="text-sm space-y-0.5">
                        <div><span class="text-gray-400">State:</span> <span class="font-mono">${voice.state || 'Unknown'}</span></div>
                        ${voice.port ? `<div><span class="text-gray-400">Port:</span> ${voice.port}</div>` : ''}
                        ${voice.arena ? `
                            <div class="pt-1 text-[11px] text-gray-400">
                                Arena: ${voice.arena.requests_since_reset || 0} / ${voice.arena.max_requests_before_reset || '?'} requests
                                ${voice.arena.uptime_seconds ? ` • Uptime: ${Math.floor(voice.arena.uptime_seconds / 60)}m` : ''}
                            </div>
                            ${voice.arena.last_reset_reason ? `
                                <div class="text-[10px] text-amber-400/90 mt-0.5">
                                    Last reset: ${escapeHtml(voice.arena.last_reset_reason)}
                                </div>
                            ` : ''}
                        ` : ''}
                    </div>
                </div>
            `;

            // Memory Pressure
            if (snapshot.memory) {
                const mem = snapshot.memory;
                html += `
                    <div class="bg-surface-light rounded-xl p-4 border border-gray-700/40 text-xs">
                        <div class="font-semibold mb-1">Memory Pressure</div>
                        <div class="text-gray-400">
                            RSS: ${mem.rss_mb || '?'} MB &nbsp;|&nbsp; VMS: ${mem.vms_mb || '?'} MB
                        </div>
                    </div>
                `;
            }

            // RAG last status (Phase C3)
            if (snapshot.rag) {
                const rag = snapshot.rag;
                const ragOk = rag.ok;
                const ragColor = ragOk === false ? 'text-amber-400' : 'text-gray-400';
                html += `
                    <div class="bg-surface-light rounded-xl p-4 border border-gray-700/40 text-xs">
                        <div class="font-semibold mb-1">Knowledge / RAG</div>
                        <div class="${ragColor}">${escapeHtml(rag.detail || '—')}</div>
                    </div>
                `;
            }

            // Orphan conversation hints (Phase A4)
            if (Array.isArray(snapshot.orphan_conversations) && snapshot.orphan_conversations.length > 0) {
                html += `
                    <div class="bg-surface-light rounded-xl p-4 border border-amber-700/40 text-xs">
                        <div class="font-semibold mb-1 text-amber-300">Orphan conversation hints</div>
                        <div class="text-gray-400 mb-1">Conversation ids that match a character id (pre-R0 path). Review manually; no auto-merge.</div>
                        <ul class="list-disc pl-4 space-y-0.5 text-gray-300">
                            ${snapshot.orphan_conversations.map(o =>
                                `<li><span class="font-mono">${escapeHtml(o.conversation_id || '')}</span> · ${o.message_count || 0} msgs · ${escapeHtml(o.name || '')}</li>`
                            ).join('')}
                        </ul>
                    </div>
                `;
            }

            // Circuit Breaker Summary
            if (snapshot.circuit_breaker && Object.keys(snapshot.circuit_breaker).length > 0) {
                html += `
                    <div class="bg-surface-light rounded-xl p-4 border border-gray-700/40 text-xs">
                        <div class="font-semibold mb-1">Circuit Breaker</div>
                        <pre class="text-[9px] font-mono bg-black/40 p-2 rounded overflow-x-auto">${JSON.stringify(snapshot.circuit_breaker, null, 2)}</pre>
                    </div>
                `;
            }

            // Reliability Summary
            html += `
                <div class="bg-surface-light rounded-xl p-4 border border-gray-700/40 text-xs">
                    <div class="font-semibold mb-1">Professional Features Active</div>
                    <div class="text-gray-400">
                        • Dual Arena Reset (requests + 45min uptime)<br>
                        • Automatic GGUF metadata parsing<br>
                        • Clean shutdown + process hygiene<br>
                        • 16 enforced chaos/invariant tests
                    </div>
                </div>
            `;

            content.innerHTML = html;
            if (window.lucide) window.lucide.createIcons();
        } catch (err) {
            content.innerHTML = `<div class="text-red-400">Failed to load diagnostics: ${err}</div>`;
        }
    })();
}

function closeDiagnostics() {
    document.getElementById('diagnosticsModal').classList.add('hidden');
}

// Wire diagnostics button (called after DOM ready)
function wireDiagnosticsUI() {
    const btn = document.getElementById('openDiagnosticsBtn');
    if (btn) {
        btn.addEventListener('click', openDiagnostics);
    }

    const closeBtn = document.getElementById('closeDiagnosticsBtn');
    const overlay = document.getElementById('diagnosticsOverlay');
    if (closeBtn) closeBtn.addEventListener('click', closeDiagnostics);
    if (overlay) overlay.addEventListener('click', closeDiagnostics);
}

// --- MOBILE SIDEBAR ---
function toggleMobileSidebar() {
    const sidebar = document.getElementById('sidebar');
    const overlay = document.getElementById('sidebarOverlay');
    const isOpen = sidebar.classList.contains('translate-x-0');

    if (isOpen) {
        closeMobileSidebar();
    } else {
        sidebar.classList.remove('-translate-x-full');
        sidebar.classList.add('translate-x-0');
        overlay.classList.remove('hidden');
        document.body.classList.add('modal-open');
    }
}

function closeMobileSidebar() {
    const sidebar = document.getElementById('sidebar');
    const overlay = document.getElementById('sidebarOverlay');
    sidebar.classList.add('-translate-x-full');
    sidebar.classList.remove('translate-x-0');
    overlay.classList.add('hidden');
    document.body.classList.remove('modal-open');
}

// Helper: Update the dynamic inference panel shown on the welcome screen
async function updateWelcomeInferencePanel(forceShow = false) {
    const panel = document.getElementById('welcomeInferenceStatus');
    const textEl = document.getElementById('welcomeInferenceText');
    const quickBtn = document.getElementById('welcomeQuickStartBtn');
    if (!panel || !textEl) return;

    const dismissed = localStorage.getItem('welcomeInferenceDismissed') === 'true';

    try {
        const status = await callTauri('get_inference_status');

        if (status && status.running) {
            panel.classList.add('hidden');
            localStorage.removeItem('welcomeInferenceDismissed');
            return;
        }

        const lastModelCount = parseInt(localStorage.getItem('lastModelCount') || '0');
        const models = await callTauri('scan_for_models') || [];
        localStorage.setItem('lastModelCount', models.length.toString());

        if (models.length === 0) {
            textEl.textContent = lastModelCount > 0
                ? 'No models detected this time. Add some .gguf files and scan again.'
                : 'No GGUF models found yet. Place models in test-models/ or ~/Models.';
            if (quickBtn) quickBtn.style.display = 'none';
            panel.classList.remove('hidden');
            return;
        }

        const best = models.find(m => m.is_vision) || models[0];
        textEl.innerHTML = `Found <span class="font-medium text-gray-200">${models.length}</span> model(s). Recommended: <span class="font-mono text-gray-300">${escapeHtml(best.filename)}</span>`;

        if (quickBtn) quickBtn.style.display = '';

        if (!dismissed || forceShow) {
            panel.classList.remove('hidden');
        } else {
            panel.classList.add('hidden');
        }
    } catch (_) {
        const last = localStorage.getItem('lastModelCount');
        if (last && parseInt(last) > 0) {
            textEl.textContent = `${last} model(s) were found previously.`;
            panel.classList.remove('hidden');
        } else {
            panel.classList.add('hidden');
        }
    }

    if (!panel.querySelector('.dismiss-welcome')) {
        const dismiss = document.createElement('div');
        dismiss.className = 'dismiss-welcome mt-2 text-right';
        dismiss.innerHTML = `<button class="text-[10px] text-gray-500 hover:text-gray-400">Hide for now</button>`;
        dismiss.querySelector('button').onclick = () => {
            localStorage.setItem('welcomeInferenceDismissed', 'true');
            panel.classList.add('hidden');
        };
        panel.appendChild(dismiss);
    }
}

// --- EVENT LISTENERS ---
function bindClick(id, handler) {
    const el = document.getElementById(id);
    if (!el) {
        console.warn('[LocalPersona] Missing element for click bind:', id);
        return null;
    }
    el.addEventListener('click', handler);
    return el;
}

let _eventListenersBound = false;

function setupEventListeners() {
    if (_eventListenersBound) return;
    _eventListenersBound = true;

    // Settings first — must work even if later binds fail
    bindClick('settingsBtn', (e) => { e.preventDefault(); openSettings(); });
    bindClick('mobileSettingsBtn', (e) => { e.preventDefault(); openSettings(); });
    bindClick('quickSettingsWelcomeBtn', (e) => { e.preventDefault(); openSettings(); });
    bindClick('closeSettingsBtn', (e) => { e.preventDefault(); closeSettings(); });
    bindClick('settingsOverlay', closeSettings);
    bindClick('saveSettingsBtn', saveSettings);
    bindClick('testConnectionBtn', () => testConnection(true));

    bindClick('mobileMenuBtn', toggleMobileSidebar);
    bindClick('sidebarOverlay', closeMobileSidebar);
    bindClick('newCharacterBtn', () => openCharacterEditor(null));

    // Mode cards + collapsible editor sections (were unbound → dead UI)
    const modeSelector = document.getElementById('modeSelector');
    if (modeSelector) {
        modeSelector.addEventListener('click', (e) => {
            const btn = e.target.closest('.mode-card[data-mode]');
            if (!btn) return;
            modeSelector.querySelectorAll('.mode-card').forEach((b) => {
                b.classList.remove('border-accent', 'bg-accent/15', 'text-accent-light');
                b.classList.add('border-transparent', 'bg-surface-light', 'text-gray-400');
            });
            btn.classList.add('border-accent', 'bg-accent/15', 'text-accent-light');
            btn.classList.remove('border-transparent', 'bg-surface-light', 'text-gray-400');
        });
    }
    const toggleWriting = document.getElementById('toggleWritingInstructions');
    if (toggleWriting) {
        toggleWriting.addEventListener('click', (e) => {
            e.preventDefault();
            const section = document.getElementById('writingInstructionsSection');
            if (!section) return;
            section.classList.toggle('hidden');
            const icon = toggleWriting.querySelector('[data-lucide], svg');
            if (icon && icon.classList) icon.classList.toggle('rotate-90');
        });
    }
    const toggleSystem = document.getElementById('toggleSystemPrompt');
    if (toggleSystem) {
        toggleSystem.addEventListener('click', (e) => {
            e.preventDefault();
            const section = document.getElementById('systemPromptSection');
            if (!section) return;
            section.classList.toggle('hidden');
            const icon = toggleSystem.querySelector('[data-lucide], svg');
            if (icon && icon.classList) icon.classList.toggle('rotate-90');
        });
    }

    // Temporary testing button for default personas
    const forceDefaultsBtn = document.getElementById('forceLoadDefaultsBtn');
    if (forceDefaultsBtn) {
        forceDefaultsBtn.addEventListener('click', async () => {
            // Phase 12.0: Replaced blocking confirm() with toast notification
            showToast('Reset to defaults: Use Settings to manage characters individually.', 'info');
        });
    }

    // === NEW: Local Inference Server Controls ===
    const startServerBtn = document.getElementById('startServerBtn');
    const stopServerBtn = document.getElementById('stopServerBtn');
    const refreshStatusBtn = document.getElementById('refreshServerStatusBtn');

    async function refreshServerStatusUI() {
        const badge = document.getElementById('serverStatusBadge');
        const detail = document.getElementById('serverStatusDetail');
        if (!badge && !detail) return;

        let status = null;
        try {
            status = await callTauri('get_inference_status');
        } catch (e) {
            console.warn('[LocalPersona] get_inference_status failed:', e);
        }

        if (!status) {
            if (badge) {
                badge.textContent = 'Backend unavailable';
                badge.className = 'text-[10px] px-2 py-0.5 rounded bg-gray-700 text-gray-400';
            }
            if (detail) detail.textContent = 'Not running inside Tauri';
            return;
        }

        currentServerStatus = status;

        if (status.running) {
            if (badge) {
                badge.textContent = 'Running';
                badge.className = 'text-[10px] px-2 py-0.5 rounded bg-emerald-500/20 text-emerald-400';
            }
            if (detail) {
                detail.textContent = `Port ${status.port} • ${status.model_path?.split('/').pop() || 'unknown model'}`;
            }
        } else if (status.starting) {
            if (badge) {
                badge.textContent = 'Starting…';
                badge.className = 'text-[10px] px-2 py-0.5 rounded bg-amber-500/20 text-amber-300';
            }
            if (detail) {
                detail.textContent = `Port ${status.port || '—'} • loading model…`;
            }
        } else {
            if (badge) {
                badge.textContent = 'Stopped';
                badge.className = 'text-[10px] px-2 py-0.5 rounded bg-gray-700 text-gray-400';
            }
            if (detail) detail.textContent = status.last_error || 'Ready to start';
        }

        // Toggle vision button visibility
        const imageBtn = document.getElementById('imageUploadBtn');
        if (imageBtn) {
            imageBtn.disabled = !status.vision_enabled;
            imageBtn.title = status.vision_enabled 
                ? 'Attach image (vision model active)' 
                : 'Vision not supported by current model';
        }
    }

    // Expose for openSettings() (nested functions are not global otherwise)
    window.refreshServerStatusUI = refreshServerStatusUI;

    if (startServerBtn) {
        startServerBtn.addEventListener('click', async () => {
            // This button now uses the new model discovery system.
            // We ask the backend for the best available test / default model.
            startServerBtn.disabled = true;
            startServerBtn.textContent = 'Scanning models...';

            try {
                // Try to get discovered models from the new command (will be added)
                const models = await callTauri('scan_for_models') || [];
                
                // Prefer a manually selected model, then vision, then any
                let chosen = null;
                if (selectedModelPath) {
                    chosen = models.find(m => m.path === selectedModelPath);
                }
                if (!chosen) {
                    chosen = models.find(m => m.is_vision) || models[0];
                }

                if (!chosen) {
                    showToast("No GGUF models found. Please place models in the 'test-models' folder or configure a model path in Settings.", 'warning');
                    startServerBtn.disabled = false;
                    startServerBtn.textContent = 'Start Local Server';
                    return;
                }

                startServerBtn.textContent = 'Starting...';

                const request = {
                    model_path: chosen.path,
                    ctx_size: 8192,
                    gpu_layers: -1,
                    flash_attn: true,
                    preferred_port: 0
                };

                if (chosen.mmproj_path) {
                    request.mmproj_path = chosen.mmproj_path;
                }

                const result = await callTauri('start_inference_server', { request });

                // start() returns immediately with starting=true, running=false until HTTP ready.
                if (result && (result.running || result.starting || result.port)) {
                    const visionNote = result.vision_enabled ? ' (Vision enabled)' : '';
                    showToast(
                        result.running
                            ? `Server started on port ${result.port}${visionNote}`
                            : `Server starting on port ${result.port}${visionNote}…`,
                        'success'
                    );
                } else {
                    showToast('Failed to start server. Check the status message or console for details.', 'error');
                }
            } catch (e) {
                showToast('Error starting server: ' + e, 'error');
            }

            startServerBtn.disabled = false;
            startServerBtn.textContent = 'Start Local Server';
            await refreshServerStatusUI();
        });
    }

    if (stopServerBtn) {
        stopServerBtn.addEventListener('click', async () => {
            await callTauri('stop_inference_server');
            await refreshServerStatusUI();
        });
    }

    if (refreshStatusBtn) {
        refreshStatusBtn.addEventListener('click', refreshServerStatusUI);
    }

    // === Voice / TTS Server Controls (Qwen3-TTS) ===
    const startVoiceBtn = document.getElementById('startVoiceServerBtn');
    const stopVoiceBtn = document.getElementById('stopVoiceServerBtn');

    if (startVoiceBtn) {
        startVoiceBtn.addEventListener('click', async () => {
            const modelPathInput = document.getElementById('ttsModelPathInput');
            const modelPath = modelPathInput ? modelPathInput.value.trim() : '';

            if (!modelPath) {
                showToast("Please specify the TTS model path in Settings.", 'warning');
                return;
            }

            startVoiceBtn.disabled = true;
            startVoiceBtn.textContent = 'Starting...';

            try {
                const request = {
                    model_path: modelPath,
                    ctx_size: 4096,
                    gpu_layers: -1,
                    flash_attn: true,
                    preferred_port: 0
                };

                const result = await callTauri('start_voice_server', { request });

                if (result && (result.running || result.starting || result.port)) {
                    showToast(
                        result.running
                            ? `Voice server started on port ${result.port}`
                            : `Voice server starting on port ${result.port}…`,
                        'success'
                    );
                } else {
                    showToast('Failed to start voice server.', 'error');
                }
            } catch (e) {
                showToast('Error starting voice server: ' + e, 'error');
            }

            startVoiceBtn.disabled = false;
            startVoiceBtn.textContent = 'Start Voice Server';
            await refreshVoiceServerStatus();
        });
    }

    if (stopVoiceBtn) {
        stopVoiceBtn.addEventListener('click', async () => {
            await callTauri('stop_voice_server');
            await refreshVoiceServerStatus();
        });
    }

    async function refreshVoiceServerStatus() {
        const badge = document.getElementById('voiceServerStatusBadge');
        try {
            const status = await callTauri('get_voice_server_status');
            if (status && status.running) {
                badge.textContent = 'Running';
                badge.className = 'text-[10px] px-2 py-0.5 rounded bg-violet-500/20 text-violet-400';
            } else {
                badge.textContent = 'Stopped';
                badge.className = 'text-[10px] px-2 py-0.5 rounded bg-gray-700 text-gray-400';
            }
        } catch (_) {
            badge.textContent = 'Not available';
            badge.className = 'text-[10px] px-2 py-0.5 rounded bg-gray-700 text-gray-400';
        }
    }

    // ============================================================
    // NEW: Model Discovery + llama-server binary management (2026)
    // ============================================================

    let discoveredModels = [];
    let selectedModelPath = null;   // Persisted selection for styling

    async function refreshDiscoveredModels() {
        const container = document.getElementById('discoveredModelsList');
        const countBadge = document.getElementById('modelCountBadge');
        const searchInput = document.getElementById('modelSearchInput');

        if (!container) return;

        container.innerHTML = '<div class="p-3 text-gray-500 text-center text-xs">Scanning for GGUF models...</div>';
        if (countBadge) countBadge.classList.add('hidden');

        try {
            const models = await callTauri('scan_for_models') || [];
            discoveredModels = models;

            // Restore previously selected path if it still exists
            if (selectedModelPath && !models.some(m => m.path === selectedModelPath)) {
                selectedModelPath = null;
            }

            renderDiscoveredModels(models, container);

            if (countBadge) {
                if (models.length > 0) {
                    countBadge.textContent = `${models.length} found`;
                    countBadge.classList.remove('hidden');
                } else {
                    countBadge.classList.add('hidden');
                }
            }

            // Re-attach live filter
            if (searchInput) {
                searchInput.oninput = () => filterModelList(searchInput.value, container);
            }
        } catch (e) {
            container.innerHTML = `<div class="p-3 text-red-400 text-xs">Failed to scan: ${escapeHtml(String(e))}</div>`;
        }
    }

    // Expose for openSettings() (nested functions are not global otherwise)
    window.refreshDiscoveredModels = refreshDiscoveredModels;

    function filterModelList(query, container) {
        if (!container) return;
        const q = (query || '').toLowerCase().trim();

        container.querySelectorAll('.model-row').forEach(row => {
            const filename = row.querySelector('.font-medium')?.textContent.toLowerCase() || '';
            const isMatch = !q || filename.includes(q);
            row.style.display = isMatch ? '' : 'none';
        });
    }

    function renderDiscoveredModels(models, container) {
        if (!container) return;

        if (!models || models.length === 0) {
            container.innerHTML = `
                <div class="p-3 text-center">
                    <div class="text-gray-400 text-xs mb-1">No GGUF models found</div>
                    <div class="text-[10px] text-gray-500">Place .gguf files in test-models/, ~/Models or ~/Downloads</div>
                </div>`;
            return;
        }

        let html = '';
        models.forEach((model, idx) => {
            const isSelected = selectedModelPath === model.path;
            const visionBadge = model.is_vision 
                ? '<span class="ml-1 px-1.5 py-px text-[9px] rounded bg-violet-500/30 text-violet-300 font-medium">VISION</span>' 
                : '';

            const sizeText = model.size_mb > 0 ? `${model.size_mb} MB` : '—';

            // New GGUF metadata display (10/10 plan - automatic GGUF understanding)
            let metaLine = '';
            if (model.architecture || model.context_length) {
                const arch = model.architecture ? model.architecture.toUpperCase() : 'Unknown';
                const ctx = model.context_length ? ` • ${model.context_length.toLocaleString()} ctx` : '';
                const params = model.parameter_count ? ` • ~${(model.parameter_count / 1e9).toFixed(1)}B params` : '';
                metaLine = `<div class="text-[9px] text-gray-500">${arch}${ctx}${params}</div>`;
            }

            const selectedClasses = isSelected 
                ? 'ring-1 ring-accent/70 bg-accent/10' 
                : 'hover:bg-surface-light';

            html += `
                <div class="model-row flex items-center justify-between px-3 py-2 cursor-pointer border-b border-gray-700/30 last:border-b-0 transition-colors ${selectedClasses}"
                     data-model-path="${escapeHtml(model.path)}" data-model-index="${idx}">
                    <div class="flex-1 min-w-0 pr-2">
                        <div class="font-medium text-gray-200 truncate flex items-center gap-1">
                            ${escapeHtml(model.filename)}
                            ${isSelected ? '<i data-lucide="check" class="w-3 h-3 text-accent flex-shrink-0"></i>' : ''}
                        </div>
                        <div class="text-[10px] text-gray-500">${sizeText}</div>
                        ${metaLine}
                    </div>
                    <div class="flex items-center gap-2 flex-shrink-0">
                        ${visionBadge}
                        <button class="use-model-btn text-[10px] px-2.5 py-0.5 bg-accent/80 hover:bg-accent text-white rounded-lg font-medium" 
                                data-idx="${idx}">Select</button>
                    </div>
                </div>`;
        });

        container.innerHTML = html;

        // Re-init lucide icons (for the checkmarks)
        if (window.lucide && lucide.createIcons) lucide.createIcons();

        // Attach handlers
        container.querySelectorAll('.model-row').forEach(row => {
            row.addEventListener('click', (e) => {
                if (e.target.classList.contains('use-model-btn')) return; // button has its own handler
                selectModelFromRow(row, container);
            });
        });

        container.querySelectorAll('.use-model-btn').forEach(btn => {
            btn.addEventListener('click', (e) => {
                e.stopImmediatePropagation();
                const row = btn.closest('.model-row');
                if (row) selectModelFromRow(row, container);
            });
        });
    }

    function selectModelFromRow(row, container) {
        const path = row.dataset.modelPath;
        const idx = parseInt(row.dataset.modelIndex);
        if (!path || isNaN(idx)) return;

        const chosen = discoveredModels[idx];
        if (!chosen) return;

        selectedModelPath = path;

        // Update model input (full path)
        const modelInput = document.getElementById('modelNameInput');
        if (modelInput) modelInput.value = chosen.path;

        // Store mmproj for later use when starting server
        if (chosen.mmproj_path) {
            window.__selectedMmprojPath = chosen.mmproj_path;
        }

        // 10/10 GGUF polish: Auto-suggest context size from parsed metadata
        const ctxInput = document.getElementById('ctxSizeInput'); // if exists in advanced settings
        if (ctxInput && chosen.context_length && chosen.context_length > 0) {
            // Only suggest if current value is default/low
            if (!ctxInput.value || parseInt(ctxInput.value) < 4096) {
                ctxInput.value = chosen.context_length;
                // Gentle hint
                const oldPlaceholder = ctxInput.placeholder;
                ctxInput.placeholder = `Suggested: ${chosen.context_length}`;
                setTimeout(() => { ctxInput.placeholder = oldPlaceholder; }, 2500);
            }
        }

        // Re-render for selected styling
        renderDiscoveredModels(discoveredModels, container);

        // Gentle success hint
        const hint = document.createElement('div');
        hint.className = 'px-3 py-1 text-[10px] text-emerald-400 bg-emerald-500/10 rounded-b-xl';
        hint.textContent = 'Model ready. You can now start the server.';
        container.appendChild(hint);
        setTimeout(() => hint.remove(), 1800);
    }

    // Wire new buttons
    const refreshModelsBtn = document.getElementById('refreshModelsBtn');
    if (refreshModelsBtn) {
        refreshModelsBtn.addEventListener('click', refreshDiscoveredModels);
    }

    // Browse for llama-server binary
    const browseLlamaBtn = document.getElementById('browseLlamaServerBtn');
    const llamaPathDisplay = document.getElementById('llamaServerPathDisplay');

    if (browseLlamaBtn) {
        browseLlamaBtn.addEventListener('click', async () => {
            try {
                const selectedPath = await callTauri('pick_llama_server_binary');

                if (selectedPath) {
                    await callTauri('set_llama_server_path', { path: selectedPath });
                    if (llamaPathDisplay) {
                        llamaPathDisplay.textContent = selectedPath;
                        llamaPathDisplay.classList.remove('text-gray-400');
                        llamaPathDisplay.classList.add('text-emerald-400');
                    }
                    showToast('llama-server binary updated successfully.', 'success');
                }
            } catch (e) {
                // Phase 12.0: Replaced blocking prompt() with toast notification
                showToast('File picker failed. Please use Settings to configure the llama-server binary path.', 'warning');
            }
        });
    }

    // Server status is refreshed inside openSettings() (see function definition)

    bindClick('startChatBtn', () => {
        if (APP_STATE.characters.length > 0) {
            selectCharacter(APP_STATE.characters[0].id);
        } else {
            openCharacterEditor(null);
        }
    });
    // quickSettingsWelcomeBtn already bound at top of setupEventListeners

    // NEW: Welcome screen quick inference start
    const welcomeQuickStartBtn = document.getElementById('welcomeQuickStartBtn');
    if (welcomeQuickStartBtn) {
        welcomeQuickStartBtn.addEventListener('click', async () => {
            welcomeQuickStartBtn.disabled = true;
            welcomeQuickStartBtn.textContent = 'Scanning...';

            try {
                const models = await callTauri('scan_for_models') || [];
                const chosen = models.find(m => m.is_vision) || models[0];

                if (!chosen) {
                    showToast("No models found. Open Settings → Discovered Models to add some.", 'warning');
                    return;
                }

                const request = {
                    model_path: chosen.path,
                    ctx_size: 8192,
                    gpu_layers: -1,
                    flash_attn: true,
                    preferred_port: 0
                };
                if (chosen.mmproj_path) request.mmproj_path = chosen.mmproj_path;

                const result = await callTauri('start_inference_server', { request });
                if (result && (result.running || result.starting || result.port)) {
                    showToast(
                        result.running
                            ? `Server started on port ${result.port}! You can now chat with characters.`
                            : `Server starting on port ${result.port}… You can chat once the model loads.`,
                        'success'
                    );
                    const panel = document.getElementById('welcomeInferenceStatus');
                    if (panel) panel.classList.add('hidden');
                }
            } catch (e) {
                showToast('Quick start failed: ' + e, 'error');
            } finally {
                welcomeQuickStartBtn.disabled = false;
                welcomeQuickStartBtn.textContent = 'Detect Models & Start Server';
            }
        });
    }

    // Simple markdown renderer for in-app About/Help (supports headings, lists, bold, code, hr)
    function renderMarkdown(md) {
        if (!md || typeof md !== 'string') {
            return '<p class="text-gray-500 italic text-xs">No content available.</p>';
        }
        // Basic HTML escape
        let text = md.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
        const lines = text.split(/\r?\n/);
        const out = [];
        let inList = false;
        let inCode = false;
        let codeBuf = [];

        for (let raw of lines) {
            const line = raw;
            const trimmed = line.trim();

            // Fenced code blocks
            if (trimmed.startsWith('```')) {
                if (inCode) {
                    const codeHtml = codeBuf.join('\n').trim();
                    out.push(`<pre class="bg-black/70 p-2.5 my-2 rounded-lg overflow-x-auto text-[11px] font-mono border border-gray-700/50 text-gray-200"><code>${codeHtml}</code></pre>`);
                    codeBuf = [];
                    inCode = false;
                } else {
                    if (inList) { out.push('</ul>'); inList = false; }
                    inCode = true;
                }
                continue;
            }
            if (inCode) {
                codeBuf.push(line);
                continue;
            }

            if (/^---+$/.test(trimmed)) {
                if (inList) { out.push('</ul>'); inList = false; }
                out.push('<hr class="my-2 border-gray-700/60">');
                continue;
            }

            // Headers # ## ###
            const hmatch = trimmed.match(/^(#{1,3})\s+(.+)$/);
            if (hmatch) {
                if (inList) { out.push('</ul>'); inList = false; }
                const lvl = hmatch[1].length;
                let c = hmatch[2]
                    .replace(/\*\*(.+?)\*\*/g, '<strong class="text-gray-100">$1</strong>')
                    .replace(/__(.+?)__/g, '<strong class="text-gray-100">$1</strong>')
                    .replace(/\*(.+?)\*/g, '<em>$1</em>')
                    .replace(/_(.+?)_/g, '<em>$1</em>')
                    .replace(/`([^`]+)`/g, '<code class="px-1 bg-black/40 rounded text-[11px]">$1</code>');
                out.push(`<h${lvl} class="font-semibold mt-2.5 mb-0.5 text-gray-100">${c}</h${lvl}>`);
                continue;
            }

            // - list items
            const lmatch = trimmed.match(/^[-*]\s+(.+)$/);
            if (lmatch) {
                if (!inList) {
                    out.push('<ul class="list-disc pl-5 my-0.5 space-y-px text-gray-300">');
                    inList = true;
                }
                let item = lmatch[1]
                    .replace(/\*\*(.+?)\*\*/g, '<strong class="text-gray-100 font-medium">$1</strong>')
                    .replace(/__(.+?)__/g, '<strong class="text-gray-100 font-medium">$1</strong>')
                    .replace(/\*(.+?)\*/g, '<em>$1</em>')
                    .replace(/_(.+?)_/g, '<em>$1</em>')
                    .replace(/`([^`]+)`/g, '<code class="px-1 py-px bg-black/50 rounded text-[11px]">$1</code>');
                out.push(`<li class="mb-px">${item}</li>`);
                continue;
            } else if (inList) {
                out.push('</ul>');
                inList = false;
            }

            // Paragraphs / body text
            if (trimmed.length > 0) {
                let p = trimmed
                    .replace(/\*\*(.+?)\*\*/g, '<strong class="text-gray-100 font-medium">$1</strong>')
                    .replace(/__(.+?)__/g, '<strong class="text-gray-100 font-medium">$1</strong>')
                    .replace(/\*(.+?)\*/g, '<em>$1</em>')
                    .replace(/_(.+?)_/g, '<em>$1</em>')
                    .replace(/`([^`]+)`/g, '<code class="px-1 py-px bg-black/50 rounded text-[11px]">$1</code>');
                out.push(`<p class="mb-1 text-gray-300">${p}</p>`);
            } else {
                out.push('<div class="h-1.5"></div>');
            }
        }
        if (inList) out.push('</ul>');
        if (inCode && codeBuf.length > 0) {
            out.push(`<pre class="bg-black/70 p-2.5 my-2 rounded-lg overflow-x-auto text-[11px] font-mono border border-gray-700/50"><code>${codeBuf.join('\n').trim()}</code></pre>`);
        }
        return out.join('');
    }

    async function showAboutModal() {
        const modal = document.getElementById('aboutModal');
        const contentEl = document.getElementById('aboutContent');
        const versionEl = document.getElementById('aboutVersionBadge');
        if (!modal || !contentEl) return;

        const info = await callTauri('get_app_info') || { version: '0.9.0-rc.1', license: 'MIT' };
        if (versionEl) {
            versionEl.textContent = `v${info.version || 'dev'}`;
        }

        const productBlurb = `
<div class="space-y-3 text-sm text-gray-300 px-1">
  <p class="text-base font-semibold text-white">LocalPersona</p>
  <p>Local-first studio for rich AI personas powered by <strong>your</strong> GGUF models and llama.cpp.</p>
  <ul class="list-disc pl-5 space-y-1 text-xs text-gray-400">
    <li><strong class="text-gray-300">Models not included</strong> — install llama-server and point Settings at your GGUF files.</li>
    <li><strong class="text-gray-300">Your data, your files</strong> — characters and chats live in your app data directory.</li>
    <li><strong class="text-gray-300">License:</strong> ${escapeHtml(info.license || 'MIT')}</li>
    <li><strong class="text-gray-300">Version:</strong> ${escapeHtml(info.version || 'dev')}</li>
  </ul>
  <p class="text-[11px] text-gray-500">RC limitations: token streaming deferred; message edit deferred; voice/TTS experimental.</p>
  <hr class="border-gray-700/60 my-2"/>
  <p class="text-[11px] text-gray-500 uppercase tracking-wide">README</p>
</div>`;

        contentEl.innerHTML = productBlurb + '<p class="text-gray-500 italic text-xs px-1">Loading README…</p>';

        try {
            const md = await callTauri('get_readme_content') || '';
            contentEl.innerHTML = productBlurb + renderMarkdown(md);
        } catch (e) {
            contentEl.innerHTML = productBlurb + `<p class="text-red-400 text-xs">Could not load README: ${escapeHtml(String(e))}</p>`;
        }

        const repo = (info.repository || '').replace(/\.git$/, '').replace(/\/$/, '');
        const viewBtn = document.getElementById('viewSourceBtn');
        const issueBtn = document.getElementById('reportIssueBtn');
        const openLink = async (url) => {
            try { await callTauri('open_external_url', { url }); } catch (_) {}
        };
        if (viewBtn) viewBtn.onclick = () => { if (repo) openLink(repo); };
        if (issueBtn) issueBtn.onclick = () => { if (repo) openLink(`${repo}/issues`); };

        if (window.lucide && typeof lucide.createIcons === 'function') lucide.createIcons();
        modal.classList.remove('hidden');
    }

    async function showHelpModal() {
        const modal = document.getElementById('helpModal');
        const contentEl = document.getElementById('helpContent');
        if (!modal || !contentEl) return;

        contentEl.innerHTML = '<p class="text-gray-500 italic text-xs px-1">Loading user manual…</p>';

        try {
            const md = await callTauri('get_user_manual_content') || '';
            contentEl.innerHTML = renderMarkdown(md);
        } catch (e) {
            contentEl.innerHTML = `<p class="text-red-400 text-xs">Could not load help content: ${escapeHtml(String(e))}</p>`;
        }

        if (window.lucide && typeof lucide.createIcons === 'function') lucide.createIcons();
        modal.classList.remove('hidden');
    }

    // About modal handlers (now renders full README inline)
    const aboutModal = document.getElementById('aboutModal');
    const closeAboutBtn = document.getElementById('closeAboutBtn');
    const aboutOverlay = document.getElementById('aboutOverlay');

    if (closeAboutBtn) {
        closeAboutBtn.addEventListener('click', () => {
            aboutModal.classList.add('hidden');
        });
    }

    if (aboutOverlay) {
        aboutOverlay.addEventListener('click', () => {
            aboutModal.classList.add('hidden');
        });
    }

    // Listen for "show-about" event from Rust menu
    if (window.__TAURI__ && window.__TAURI__.event) {
        window.__TAURI__.event.listen('show-about', () => {
            showAboutModal();
        });
    }

    // NEW: Dedicated Help window (in-app, no external file)
    const helpModal = document.getElementById('helpModal');
    const closeHelpBtn = document.getElementById('closeHelpBtn');
    const helpOverlay = document.getElementById('helpOverlay');

    if (closeHelpBtn) {
        closeHelpBtn.addEventListener('click', () => {
            helpModal.classList.add('hidden');
        });
    }
    if (helpOverlay) {
        helpOverlay.addEventListener('click', () => {
            helpModal.classList.add('hidden');
        });
    }

    if (window.__TAURI__ && window.__TAURI__.event) {
        window.__TAURI__.event.listen('show-help', () => {
            showHelpModal();
        });
    }

    // Character editor (primary binds also near end via bindClick — avoid double-bind here)
    bindClick('deleteCharBtn', () => {
        const editingId = document.getElementById('saveCharBtn')?.dataset?.editingId;
        if (editingId) deleteCharacter(editingId);
    });

    const colorPicker = document.getElementById('colorPicker');
    if (colorPicker) {
        colorPicker.addEventListener('click', (e) => {
            const btn = e.target.closest('button[data-color]');
            if (!btn) return;
            colorPicker.querySelectorAll('button').forEach((b) => {
                b.classList.remove('ring-2', 'ring-white/20', 'ring-offset-2', 'ring-offset-surface');
            });
            btn.classList.add('ring-2', 'ring-white/20', 'ring-offset-2', 'ring-offset-surface');
        });
    }

    bindClick('uploadAvatarBtn', () => {
        document.getElementById('avatarFileInput')?.click();
    });

    const charAvatarUrlInput = document.getElementById('charAvatarUrlInput');
    if (charAvatarUrlInput) {
        charAvatarUrlInput.addEventListener('input', (e) => {
            updateAvatarPreview(e.target.value);
        });
    }
    const avatarFileInput = document.getElementById('avatarFileInput');
    if (avatarFileInput) {
        avatarFileInput.addEventListener('change', (e) => {
            const file = e.target.files[0];
            if (!file) return;
            if (file.size > 2 * 1024 * 1024) {
                showToast('Image too large. Please choose an image under 2MB.', 'warning');
                e.target.value = '';
                return;
            }
            const reader = new FileReader();
            reader.onload = (ev) => {
                const dataUrl = ev.target.result;
                const urlInput = document.getElementById('charAvatarUrlInput');
                if (urlInput) urlInput.value = dataUrl;
                updateAvatarPreview(dataUrl);
            };
            reader.readAsDataURL(file);
        });
    }
    bindClick('clearAvatarBtn', () => {
        const urlInput = document.getElementById('charAvatarUrlInput');
        const fileInput = document.getElementById('avatarFileInput');
        if (urlInput) urlInput.value = '';
        if (fileInput) fileInput.value = '';
        updateAvatarPreview('');
    });

    // === Vision: Image upload wiring ===
    const imageUploadBtn = document.getElementById('imageUploadBtn');
    const imageFileInput = document.getElementById('imageFileInput');
    const attachFileBtn = document.getElementById('attachFileBtn');

    const onAttachImage = async () => {
        if (!currentServerStatus?.vision_enabled) {
            showToast('Current model does not support vision. Load a VLM model first.', 'warning');
            return;
        }
        await handleImageUpload();
    };

    if (imageUploadBtn && imageFileInput) {
        imageUploadBtn.addEventListener('click', onAttachImage);

        // Keep the hidden input for potential drag & drop later
        imageFileInput.addEventListener('change', (e) => {
            if (e.target.files.length > 0) {
                // For now we still use the dialog, but this is ready for future
                console.log('File input changed (future drag & drop)');
            }
            e.target.value = '';
        });
    }
    if (attachFileBtn) {
        attachFileBtn.addEventListener('click', onAttachImage);
    }

    document.getElementById('exportAllBtn').addEventListener('click', exportAllCharacters);
    document.getElementById('importBtn').addEventListener('click', async () => {
        const jsonString = await callTauri('import_characters');
        if (!jsonString) return;

        try {
            const data = JSON.parse(jsonString);
            if (Array.isArray(data)) {
                // Merge into existing roster (do not wipe contacts already on disk)
                let ok = 0;
                for (const raw of data) {
                    const char = normalizeCharacter(raw);
                    if (!char.id) char.id = `imported-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
                    if (APP_STATE.characters.find(c => c.id === char.id)) {
                        char.id = `imported-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
                    }
                    const saved = await saveCharacterToDisk(char);
                    if (saved) {
                        APP_STATE.characters.push(char);
                        ok += 1;
                    } else {
                        showToast(`Failed to save contact "${char.name || char.id}" — skipping`, 'error');
                    }
                }
                if (!APP_STATE.activeCharacterId && APP_STATE.characters.length > 0) {
                    APP_STATE.activeCharacterId = APP_STATE.characters[0].id;
                    saveToStorage('activeCharacterId', APP_STATE.activeCharacterId);
                }
                renderCharacterList();
                renderQuickSwitchCards();
                await updateChatView();
                showToast(`Imported ${ok} of ${data.length} contacts`, ok > 0 ? 'success' : 'error');
            } else if (data && typeof data === 'object' && data.id) {
                const char = normalizeCharacter(data);
                if (APP_STATE.characters.find(c => c.id === char.id)) {
                    char.id = `imported-${Date.now()}`;
                }
                const saved = await saveCharacterToDisk(char);
                if (saved) {
                    APP_STATE.characters.push(char);
                    renderCharacterList();
                    renderQuickSwitchCards();
                    showToast('Contact imported!', 'success');
                } else {
                    showToast('Failed to save contact to disk', 'error');
                }
            } else {
                showToast('Invalid JSON format for characters.', 'error');
            }
        } catch (err) {
            showToast('Failed to parse JSON file.', 'error');
        }
    });

    document.getElementById('exportSingleCharBtn').addEventListener('click', () => {
        const editingId = document.getElementById('saveCharBtn').dataset.editingId;
        const char = APP_STATE.characters.find(c => c.id === editingId);
        if (char) exportSingleCharacter(char);
    });

    document.getElementById('cancelDeleteBtn').addEventListener('click', closeConfirmDelete);

    bindClick('sendBtn', sendMessage);
    bindClick('regenerateBtn', regenerateLastResponse);

    const chatInput = document.getElementById('chatInput');
    if (chatInput) {
        chatInput.addEventListener('keydown', (e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                // Phase C5: block double-send while generating
                if (!APP_STATE.isGenerating) {
                    sendMessage();
                }
            }
        });

        chatInput.addEventListener('input', () => {
            chatInput.style.height = 'auto';
            chatInput.style.height = Math.min(chatInput.scrollHeight, 128) + 'px';
            const len = chatInput.value.length;
            const hint = document.getElementById('charCountHint');
            if (!hint) return;
            if (len > 100) {
                hint.textContent = `${len} / 8000`;
                hint.classList.remove('hidden');
            } else {
                hint.classList.add('hidden');
            }
        });
    }

    // Character editor binds (null-safe)
    bindClick('closeCharEditorBtn', closeCharacterEditor);
    bindClick('charEditorOverlay', closeCharacterEditor);
    bindClick('saveCharBtn', saveCharacter);

    document.getElementById('temperatureSlider').addEventListener('input', (e) => {
        document.getElementById('tempValue').textContent = parseFloat(e.target.value).toFixed(2);
    });
    document.getElementById('topPSlider').addEventListener('input', (e) => {
        document.getElementById('topPValue').textContent = parseFloat(e.target.value).toFixed(2);
    });

    // R2-A: streaming frozen — ignore clicks; keep toggle off
    document.getElementById('streamToggle')?.addEventListener('click', (e) => {
        e.preventDefault();
        showToast('Streaming is disabled in this RC. Responses arrive as complete messages.', 'info');
        APP_STATE.settings.stream = false;
    });

    document.addEventListener('keydown', (e) => {
        if (e.key === 'Escape') {
            closeCharacterEditor();
            closeSettings();
            closeConfirmDelete();
            closeMobileSidebar();
        }
        if ((e.ctrlKey || e.metaKey) && e.key === 'k') {
            e.preventDefault();
            document.getElementById('chatInput').focus();
        }
    });

    window.addEventListener('resize', () => {
        if (window.innerWidth >= 1024) {
            closeMobileSidebar();
        }
    });

    // Delegated click handler for images in messages (replaces inline onclick)
    document.getElementById('messagesContainer')?.addEventListener('click', (e) => {
        const img = e.target.closest('.message-images img');
        if (img && img.src) {
            window.open(img.src, '_blank');
        }
    });
}

// --- LIGHTWEIGHT TOAST / NOTIFICATION SYSTEM ---
function showToast(message, type = 'error', timeoutMs = 4500) {
    const existing = document.getElementById('lp-toast-container');
    const container = existing || (() => {
        const c = document.createElement('div');
        c.id = 'lp-toast-container';
        c.style.cssText = 'position:fixed;bottom:20px;left:20px;z-index:99999;display:flex;flex-direction:column;gap:8px;';
        document.body.appendChild(c);
        return c;
    })();

    const colors = {
        error: '#b91c1c',
        success: '#166534',
        info: '#1e40af',
        warning: '#854d0e'
    };

    const toast = document.createElement('div');
    toast.textContent = message;
    toast.style.cssText = `
        background:${colors[type] || colors.error};
        color:white;
        padding:12px 16px;
        border-radius:8px;
        box-shadow:0 4px 12px rgba(0,0,0,0.3);
        font-size:14px;
        max-width:420px;
        pointer-events:auto;
        cursor:pointer;
        line-height:1.35;
    `;
    toast.onclick = () => toast.remove();

    container.appendChild(toast);

    setTimeout(() => {
        if (toast.parentNode) toast.parentNode.removeChild(toast);
    }, timeoutMs);
}

// --- TAURI INTEGRATION HELPERS (new) ---
function isTauri() {
    return typeof window !== 'undefined' && window.__TAURI__ !== undefined;
}

async function callTauri(command, payload = {}) {
    if (!isTauri()) {
        console.warn('[LocalPersona] Not running inside Tauri — command ignored:', command);
        return null;
    }
    try {
        // Tauri 2 way
        return await window.__TAURI__.core.invoke(command, payload);
    } catch (err) {
        console.error('[LocalPersona] Tauri invoke failed:', command, err);
        const msg = (err && (err.message || err.toString())) || 'Unknown Tauri error';
        showToast(`Error in ${command}: ${msg}`, 'error');
        throw err; // Propagate so callers can react (instead of silently returning null)
    }
}

// Expose a global for debugging
window.LocalPersona = {
    isTauri,
    callTauri,
    startServer: (modelPath) => callTauri('start_inference_server', {
        request: {
            model_path: modelPath,
            ctx_size: 8192,
            gpu_layers: -1,
            flash_attn: true
        }
    }),
    stopServer: () => callTauri('stop_inference_server'),
    getServerStatus: () => callTauri('get_inference_status'),
    updateWelcomeInferencePanel,
};

// --- INITIALIZE ---
document.addEventListener('DOMContentLoaded', initApp);
