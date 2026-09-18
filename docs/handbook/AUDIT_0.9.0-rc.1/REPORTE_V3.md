# Auditoría Técnica Integral v3.0 — LocalPersona 0.9.0-rc.1
*Core + Audit Variants. Evidencia del slice no-GPU reutilizada (F-001..F-022).*

## Contexto del Proyecto

| Campo | Valor |
|---|---|
| Nombre | LocalPersona 0.9.0-rc.1 (`com.localpersona.studio`, MIT) |
| Tipo | Desktop (Tauri 2: Rust + vanilla JS, sin bundler) |
| Dominio | Profesional/Consumer creativo (character studio local) |
| Estado actual | RC (closed-beta condicionada; madurez ~8.7–9.0, soak humano abierto) |
| Usuarios objetivo | Mixto (creadores no técnicos + usuarios técnicos BYO-modelo) |
| Plataformas | Linux (.deb ~14MB primario; AppImage degradado; sin modelos ni binario incluidos) |
| Stack | Rust 2021 + tokio, Tauri 2 IPC, HTTP OpenAI-compatible a `llama-server` hijo (BYO), fastembed AllMiniLML6V2, ficheros JSON+NDJSON (sin DB) |
| Despliegue | `.deb` vía `scripts/package-release.sh`; gate local `scripts/tribunal.sh` + hook pre-commit |
| Contexto ejecución | GUI interactivo, offline-first por diseño, sin telemetría |
| Compliance | Ninguno formal |

### Módulos activos

- [x] **CORE** · [x] **VAR-UX** · [x] **VAR-SEC** · [x] **VAR-PERF** · [x] **VAR-PRIV** · [x] **VAR-DEVOPS** · [x] **VAR-AI** · [x] **VAR-API** (lite: IPC Tauri + HTTP localhost) · [x] **VAR-A11Y** (lite, sin screen-reader manual) · [ ] **VAR-MOB** (no aplica)

Evidencia base (re-ejecutada para este reporte): `cargo check` verde (26 warnings dead-code; clippy `--all-targets` **67** warnings tras 4 fixes aplicados); `destructive_tests` 30/30; `property_tests` 18/18; `ctx_budget_property` 8+1 (5 witnesses green = defectos pineados); `gguf_roster_probe` 2/2 (A/B mixto: Rogue `Some(131072)` → budget 72,089 vs server 8192 — **F-001 EN VIVO**; gemma3/qwen35 → fallback 4500); mock graft smoke OK (`/ctl`, `/tokenize`, `usage` computado, `ctx400`→400 accionable). Registro canónico: `docs/handbook/AUDIT_0.9.0-rc.1/FINDINGS_REGISTER.md` (F-001..F-022). Regla de unknowns: lo no evidenciado figura `UNVERIFIED`, nunca estimado. Sesión conductual GUI pendiente (F-022) — sus filas figuran 🟠 con owner explícito.

---

# PARTE I — CORE

## 1. Arquitectura & Diseño — ⚠️ (sólida, con 1 defecto vivo)

Diagrama y contratos en `ARCHITECTURE_AND_STATUS.md` + `PHASE3_SPEC.md`. Separación real: `commands.rs` (hub IPC) → `storage.rs`/`conversation.rs` (persistencia) → `inference.rs` (dual managers) → hijos `llama-server`; RAG (`rag.rs`+`gguf.rs`) solo lectura en el path de inferencia. Testabilidad: parse/compose puros extraídos (`parse_llm_response`, `apply_truncation_marker`, `compose_system_prompt`) + harnesses. Deuda viva: **F-001** — el budget (`commands.rs:597-604`, GGUF×0.55) y el server (`--ctx-size` hardcode 8192, `frontend/script.js:2969,3324`) usan fuentes distintas de ctx; Rogue dispara 72k vs 8k hoy. Fix pineado en `PHASE3_SPEC.md` (fuente única + reservas + clamp; 0.55 muere). Acoplamiento aceptado y documentado: `SharedLlamaServer` Mutex global (nota Fase 3 en `inference.rs:1304`, futuro actor-model).

## 2. Ciclo de vida & Ghosting — ✅ con 1 confirmación pendiente

`ServerState` explícito (Idle/Starting/Running/Stopping/Error/Restarting); start retorna inmediato con `starting:true`, readiness por polling `/v1/models` en background + timeout fijo 15s/10s (F-010: sin escalado por tamaño — S2). PID files + `Drop::start_kill` + cierre de ventana detiene ambos managers (higiene verificada en código; soak A7 pendiente de sesión). `wait_for_server_ready` + `check_health`/`try_wait` antes de HTTP caro. stdout post-ready sin drenar (F-003, fix 3 líneas pendiente, trigger estrecho: builds verbose). Sin tray-icon (no aplica 2a más allá de: cierre = quit real + kill hijos).

## 3. Resource Locking & Instancia única — ⚠️

`atomic_write` (temp+rename+fsync) + `fs2` exclusive locks en metadata/ndjson/characters; sidecar locks sin unlink por mtime (diseño explícito, claim "stale 30s" inexistente → F-015 docs). **Sin single-instance guard** (F-005, S2: fs2 mitiga corrupción cross-process; riesgo residual = conflicto de puerto + split-brain UI). Sin WAKEUP (no hay 2ª instancia que despertar hasta el plugin).

## 4. Source of Truth — ⚠️ (1 divergencia conocida)

Conversaciones/personas = ficheros (SoT real). Divergencia presupuestaria F-001 (budget≠server ctx) y pill JS (`.length` UTF-16 vs byte-len Rust, F-016 — gate ±15% vía `usage.prompt_tokens` pendiente). UI bloquea switch mid-generation (`script.js:1229-1230` + token de generación 2023/2054) — sin split-brain de render.

## 5. Silent init failures — ✅

RAG fail-closed con log y chat que continúa (`rag.rs:95-97,14-18`); menús emiten con `log::warn` en fallo; `resolve_binary` con cascada override→resources→PATH→dirs comunes; GGUF ilegible → fallback + warn (el fallback es numéricamente defectuoso = F-001, pero nunca silencioso).

## 6. GUI vs headless — ✅ (marco correcto, 1 asterisco)

Sin dependencia DISPLAY/DBUS en backend; logging file/stderr-friendly; mock drop-in permite headless testing del manager lifecycle. Asterisco documentado: Tailwind/fonts vía CDN requieren red para estilo completo (offline-by-design con asterisco visual).

## 7. Bugs & Non-happy paths — ⚠️ (ver §B y Parte IV)

Cobertura mecanizada: C-matrix (13 modos, mock+ctl), B4 partial-line, paginación vs budget-walk (offset consume líneas corruptas vs walk las salta — seam documentado), image caps (10 imgs/10MB/5MB c/u), `validate_localhost_endpoint` (solo loopback). Huecos vivos: F-001 (EN VIVO), reservas F-002, clamp F-007, proxy F-004, T-9 round-trip solo-JSON (F-006), repair sin trigger UI (F-010/T-10 parcial). Cero `TODO/FIXME` en `src/*.rs` + `script.js` (grep 2026-09-17).

## 8. Estabilidad — ⚠️ (infra lista, soak pendiente)

Arena 300 req + 45min uptime en ambos managers + monitor RSS/VMS con canal de reset; circuit_breaker + autopsy como scaffolding no conectado (F-021: decisión conectar-o-eliminar abierta, ~465 LOC). Retry: 1 auto-restart + 1 reintento HTTP en `post_chat_completion` (acotado, sin backoff — aceptable para worker local). Soak 8h + chaos A1/A3: harness listo (`soak.sh`/`soak_driver.py`), ejecución pendiente → gate RC.

## 9. Usabilidad básica — ✅ (con 2 diferidos RC)

Toast no-intrusivo en todo error IPC (`callTauri` throw + toast); `isGenerating` deshabilita send (`script.js:689`) y bloquea cambio de contacto con aviso; `confirm()` en regen-from destructivo (`:2168`); streaming congelado y edición oculta con mensajes explícitos (RC declarado, no callejones). Error ctx400 aún sin ensayar en UI (fila de sesión).

## 10. Seguridad básica — ✅ (superficie mínima verificada)

`grep password|api_key|secret` en `src/` (excl. sampling/tokens): **0 hits**. Exec externo: solo 2× `Command::new` (spawn llama-server, argv sin shell) + 1× `open::that` con validación http/https (Fase 1). Path traversal endurecido (`..` strip + `starts_with(base)`); IDs ASCII sin slashes; localhost-only + `redirect(Policy::none)` + timeout 120s (F-012: partir en connect/total). T-4 proxy (`.no_proxy()` ausente) = finding abierto S1. Sin auth (app local monousuario — coherente con threat model).

## 11. Observabilidad — ✅

`env_logger` + niveles usados (`warn` en fallos setup, `debug` líneas del hijo, `info` spawn/ready); `get_diagnostics_snapshot` (ServerState, Arena + razones, memoria, RAG, huérfanos); mock `hitlog` + `mock_requests.jsonl` (payloads completos) + tap `LOCALPERSONA_CAPTURE_PROMPTS` (request + `usage` + `finish_reason`). Correlación por intento (attempt loop) básica pero suficiente. Gap: pill vs `usage` (F-016, mecánico).

## 12. Distribución — ✅ (con AppImage degradado conocido)

`.deb` 14M como primario + binario + AppDir; `bundle.resources` solo personas/manual/avatares (verificado: sin GGUF, script rehúsa build si aparecen); `INSTALL.md`/`PACKAGING.md` vigentes; desinstalación + ghost-check en checklist RC (P-3, pendiente sesión T-8). Deuda: sin remote CI (F-020 residual) y AppImage falla en este host (documentado, no bloqueante).

## 13. Dependencias externas — ⚠️ (2 findings vivos)

llama-server BYO sin versionado mínimo (F-011 S2: `--version` probe + línea en INSTALL pendientes). fastembed descarga HF en primer uso (F-009 S2: fail-closed verificado en código, evidencia conductual T-1 pendiente, secuencia bloqueado→abierto en runbook). Timeouts: total 120s sí, connect dedicado no (F-012). `Cargo.lock` commiteado; `cargo audit` nunca ejecutado → hueco registrado (no invento veredicto supply-chain).

## 14. Regresión & contratos — ✅ (con F-014 residual)

Tribunal (`check` + 30 destructive + 18 property) en hook pre-commit, medido 6–11s warm; golden IPC schema + drift test; witnesses CTX + probe A/B pineados (relaciones post-fix, regla anti-test-decorativo). Residual: `command_layer`/`stress` solo en CI (inexistente sin remote) → F-014 estrechado a contenido-CI, salida compartida con F-020.

## 15. Documentación — ⚠️ (post-limpieza F-015)

Canónico `ARCHITECTURE_AND_STATUS.md` + índice + `PHASE3_SPEC.md` + runbooks + manifest + `USER_MANUAL.md` shipeado. F-015 corrigió 0.82/8192, "16 tests" (58 reales), stale-lock inexistente. Regla: docs = ground truth; este reporte no duplica el registro, lo roll-upea.

## 16. Calidad & mantenibilidad — ✅ (tendencia controlada)

71→67 warnings clippy tras 4 fixes mecánicos (verificados + testeados); dead-code = scaffolding R3 con decisión abierta (no `allow`); 0 TODOs; build reproducible (`Cargo.lock`, tribunal). Onboarding: `AGENTS.md` + blueprint permiten contribuir en días.

## 17. Caché & persistentes — ✅

Sin caché recurrente sin dueño: PID files con cleanup en `stop()` + detección stale; `.bak` de regen/repair acotados por conversación; `mock_requests`/captures/soak.csv ignorados en git; `du`-monitoreo en soak. Disco lleno durante append: `UNVERIFIED` — fila propuesta (no inventada): inyectar ENOSPC vía mock fs en `atomic_write` y comprobar error accionable sin corrupción.

---

# PARTE II — VARIANTS

## VAR-UX — ⚠️ (mecánica sana, evidencia de matriz pendiente)

Nielsen (1–5, con cita): H1 4 (ServerState + toasts + pill), H2 4 (lenguaje "Add Contact", no jerga), H3 4 (back/undo: regen con confirm + backup; streaming/edit declarados fuera de RC), H4 4 (componentes consistentes, `escapeHtml` sistemático), H5 3 (guards + confirm; falta clamp-feedback F-013 y recovery guiado de overflow), H6 4 (sidebar messenger + previews), H7 3 (sin atajos/power-features — aceptable RC), H8 4, H9 3 (errores humanos y accionables en su mayoría; ctx400/toast pendiente de ensayo), H10 4 (manual shipeado + contextuales). UX-5: loading (isGenerating+disabled), error (toast+estados server), vacío, deshabilitado y offline-diseñado cubiertos; **background-progreso** parcial (readiness sin % — menor). Formularios: validación con toast junto a contexto, sin pérdida (save-then-upload), keyboard `UNVERIFIED` (spot-check Tab en sesión, 10 min). Microcopía: verbos claros ("Save the contact first…"), tono consistente EN; i18n ausente (aceptado RC, anotado). Dark patterns (UX-8): checklist limpio — sin roach-motel/confirmshaming/costes ocultos/suscripciones (no hay monetización ni cuentas). Onboarding: TTV ≈ instalar + señalar binario + modelo; zero-state con defaults (7 personas).

## VAR-SEC (lite-profundizado, sin pentest) — ⚠️

STRIDE (3 boundaries): **IPC Tauri** (S: n/a local; T: schemas golden + drift test; I: sin secretos en payloads — grep limpio); **HTTP localhost→llama-server** (T: `Policy::none` verifica no-follow conductualmente; I: T-4 proxy = egress accidental de prompts en laptops corporativas — S1, fix `.no_proxy()`; D: timeout 120s + readiness); **parsers de fichero** (GGUF/PDF/imagen/base64 con caps 100MB/10MB/5MB + `infer`; traversal bloqueado). Sin auth/authz (coherente: monousuario local, sin boundary de privilegio). Supply chain: veredicto diferido a `cargo audit` (hueco explícito). Headers web: N/A (CSP Tauri presente y correcta). Detección: logs de muerte de worker + OOM paths; sin SIEM (fuera de scope).

## VAR-PERF — ⚠️ (sin baseline = finding, no número)

PERF-1: **no existen benchmarks/P50-P99/SLOs** → finding (medir durante soak: latencia p50/p95 del driver ya instrumentada). Frontend: single-file JS ~3.3k LOC sin framework, lazy avatars, paginación NDJSON (regresión de carga completa ya corregida). Backend: token-budget walk O(historia), RAG top-8 con cold-start documentado, HTTP `stream:false` (memoria acotada por respuesta salvo modo `big` = fila de sesión). Límites: Arena + caps + clamps; cancelación de generación en curso: `UNVERIFIED` (¿abort entre turnos? — fila propuesta). Escalado: monousuario local, N/A por diseño.

## VAR-PRIV — ⚠️ (privado por arquitectura, no certificado)

Inventario: personas/chats/knowledge/voz = ficheros locales del usuario; telemetría = ninguna; red = HF-embeddings (primer uso RAG) + CDN estilos + riesgo proxy (T-4). Minimización OK por construcción. Derechos: export JSON (parcial, F-006 binarios), borrado por character con cascada (T-3 verificado en código), sin portal de acceso/portabilidad formal → puntaje honesto "respetuoso, no GDPR-certificado". Cookies/tracking: N/A (desktop sin webviews remotas salvo CDN). PIA (Anexo E): peor caso = prompts al proxy corporativo (T-4) y exfiltración de knowledge vía RAG a un mock — ambos mitigados por `.no_proxy()` + localhost-only.

## VAR-DEVOPS — ⚠️ (local excelente, remoto inexistente)

Pipeline local: Tribunal + hook medido + tags (`dd295a4`, baseline) + releases en `dist/`. Entornos: dev-vs-`.deb` smoke pendiente (T-8, fila de sesión). Deploy/rollback: instalador versionado + git tags; migraciones de formato con `version` + repair con backup (rollback de datos por conversación). Observabilidad: logs + Diagnostics, sin APM (coherente offline). Secrets prod: N/A (sin secretos). Brecha: remote/CI (F-020) — el fast/full split vive ahí.

## VAR-API (lite) — ✅

IPC Tauri: 5 schemas golden + drift test (contrato versionado de facto); errores como `Err(String)` accionables; sampling camelCase→snake mapeado y testeado. HTTP al hijo: contrato OpenAI-compatible mínimo (`/chat/completions`, `/v1/models`, `/tokenize` en mock) con shape-robustness (empty/wrongshape/choices-vacío → rechazo explícito, filas de sesión). Versionado de API propia: `version` en metadata/persona. DX: N/A público (API interna).

## VAR-A11Y (lite) — ⚠️ con plan de cierre barato

Automático verificado: `alt=""` decorativos con fallback de iniciales, `loading="lazy"`, semántica HTML base. Pendiente (no afirmado): Tab-order completo, foco visible, contraste AA medido, `prefers-reduced-motion`, NVDA/VoiceOver smoke (15–30 min en sesión, checklist en runbook propuesto). Sin gestos complejos ni time-outs hostiles. Sin texto crítico solo-color (toasts tipados + texto).

## VAR-AI — ⚠️ (el track con más deuda abierta, toda calendarizada)

AI-1: sin métricas de fidelidad medidas — QA-6 battery + scorecard (gate ≥4.0, ninguna dim <3) lista en `harness/`, ejecución Tier-2. AI-2: alucinaciones — mitigación parcial (RAG con títulos de fuente + `system_prompt` override exacto); sin detector de confianza → 🟠 con QA-5/QA-6 como cierre. AI-3: sesgos no evaluados → 🟠 (batería adversarial incluida en `fidelity_prompts.json`). AI-4: prompt-injection de docs (QA-5 "ignore previous instructions") con comportamiento por documentar → 🟠. AI-5: N/A económico (inferencia BYO local); rate-limit = Arena; sin sorpresas de facturación posibles. **Nota estructural**: F-001 silencioso (truncamiento izquierdo come `system` = persona vaciada) es el peor modo IA del sistema — por eso es EN VIVO/fix-before-beta.

---

# ANEXOS

## A — Integración & circulares

`commands ↔ inference` vía `SharedLlamaServer` (documentado, con dirección de refactor); `storage ↔ conversation` unidireccional por paths; frontend→backend solo por `invoke` tipado. Sin ciclos de módulo detectados (`rg use crate::` inspection 2026-07). Excepción honesta: `main.rs`+`lib.rs` dual-root con `mod capture` duplicado (harness temporal, con remoción calendarizada post-sesión o promoción a definitivo).

## B — Bugs (del registro; severidad | ubicación | fix)

S1: F-001 EN VIVO (budget vs server; `commands.rs:597`/`inference.rs:379`; PHASE3_SPEC) · F-002 reservas (mismo lote) · F-004 proxy (`commands.rs:36`, `.no_proxy()`) · F-006 round-trip binarios. S2: F-003 stdout-drain · F-005 single-instance · F-007 clamp-ctx · F-008 parser (gated F-001) · F-009 fastembed · F-010 readiness · F-011 `--version` · F-012 timeouts · F-013 clamp-feedback · F-014 tribunal-full · F-019 orden · F-020 remote/CI. S3: F-015 docs · F-016 pill · F-017 manifest(remediado) · F-018 regen (expected-pass) · F-021 R3 · F-022 ownership/provenance (proceso, mitigado con header gate). Candidatas a fila dura de sesión: append-on-failure, redirect-follow, hang>120s, B4-concatenated (triage inmediato si aparecen).

## C — Edge checklist (estado)

Residuo testeable restante: ENOSPC en `atomic_write`, cancelación mid-generation, Tab-order, `max_tokens` 50000 (S2, calendarizada), import-as-new vs wipe (T-9), pill ±15% (F-016), `usage` vs estimador (Tier-2). Todo lo demás del Anexo C del template: cubierto (locks con fs2, SoT ficheros, safety blocks, headless-OK, IDOR N/A local, sin queries DB).

## D — Threat model (resumen)

Actores: usuario local, red local/proxy corporativo, ficheros GGUF/PDF/imagen no confiables, HF (embeddings). Entradas: `send_message_with_images` (+base64), knowledge docs, import JSON, GGUF paths, env proxy. Boundaries: Tauri IPC (schemas), loopback HTTP (validate+no-redirect), fs sandbox (base+ASCII ids). Datos sensibles: prompts/personas/chats (reposo local, tránsito loopback salvo T-4). Mitigaciones por boundary en §SEC-2/5 + findings abiertos donde faltan (T-4, F-011).

## E — PIA lite

Tratamientos: todo local-first. Riesgo significativo único: egress accidental vía proxy (T-4, S1, fix puntual) y descarga de modelo de embeddings (consentimiento implícito pobre → F-009: vendorizar u opt-in explícito). Borrado: por personaje con cascada; backups `.bak` por conversación (retención implícita — anotar en manual post-RC).

---

# PARTE IV — RESULTADOS

## 🔴 Críticos (bloquean beta abierta)

| # | Módulo | Problema | Ubicación | Impacto | Fix urgente |
|---|---|---|---|---|---|
| 1 | CORE/AI | F-001 EN VIVO: budget 72,089 vs server 8192 en Rogue | `commands.rs:597-604`, `inference.rs:379-381` | 400 o persona vaciada en chats largos con cualquier llama-ctx≥14,895 | PHASE3_SPEC (fuente+reservas+clamp) antes de beta |
| 2 | SEC | T-4 proxy hijack de localhost | `commands.rs:36-39` | Fallo total + egress de prompts en laptops corporativas | `.no_proxy()` + connect timeout |

## 🟠 Importantes (pre-release / sesión)

F-002 (reservas, mismo lote F-001) · F-006 (binarios en export) · F-007 (clamp-ctx) · F-003 (drain, fix-ya 3 líneas) · F-009 (fastembed: evidencia T-1 en sesión) · F-010/F-012 (readiness/timeouts) · F-011 (min-version) · F-013 (clamp-feedback) · QA-6/AI-1..4 (Tier-2) · soak 8h · T-8 smoke `.deb` · A11Y smoke · PERF-1 baseline · `cargo audit` · cancel-mid-generation · ENOSPC.

## 🟡 Deseables (roadmap)

F-005 single-instance · F-008 parser (post-F-001, primer commit) · F-014/F-020 remote CI · F-021 R3 conectar-o-eliminar · streaming real · edit NDJSON · AppImage · vendor Tailwind/fonts · `.d.ts` del schema IPC · i18n.

## 🟢 Fortalezas

Atomic writes + fs2 + fsync · PID hygiene + Drop-kill · append-only NDJSON + repair con backup · `Policy::none` + localhost-only + traversal-hardening · cero secretos en código · `escapeHtml` sistemático · RAG fail-closed · regen atómico · truncation marker · Tribunal en hook (6–11s) + goldens + witnesses que pinean relaciones · gguf probe empírico · mock/ctl/`usage` harness · model manifest.

## Veredicto por módulo

| Módulo | Estado | Justificación |
|---|---|---|
| CORE arquitectura/ciclo/estabilidad | ⚠️ | Sólida + F-001 vivo + soak pendiente |
| CORE seguridad básica/observabilidad | ✅ | Superficie mínima verificada, diagnósticos reales |
| VAR-UX | ⚠️ | Mecánica sana; matriz de estados/errores pendiente de sesión |
| VAR-SEC | ⚠️ | T-4 abierto; resto mitigado o N/A coherente |
| VAR-PERF | ⚠️ | Sin baseline; cotas arquitectónicas sanas |
| VAR-A11Y (lite) | ⚠️ | Base sana; smoke manual pendiente |
| VAR-PRIV | ⚠️ | Privado por diseño; sin certificación ni PIA formal |
| VAR-API (lite) | ✅ | Contratos golden + robustez de shapes |
| VAR-DEVOPS | ⚠️ | Local excelente; remoto inexistente |
| VAR-AI | ⚠️ | Deuda calendarizada (QA-6/Tier-2); F-001 es su peor modo |

## Veredicto operativo

| Dimensión | Estado | Justificación |
|---|---|---|
| Uso real (beta cerrada) | ⚠️ Condicionado | Sí tras Phase-3 (F-001+reservas) + sesión sin filas duras |
| Publicación pública | ❌ No | F-001 EN VIVO + T-4 + QA/soak pendientes |
| Mantenimiento largo plazo | ✅ Sí | Registro, specs, harnesses, Tribunal, manifest |
| Escalado futuro | ✅ Sí | Monousuario local; sin deuda de escala |
| Entorno degradado | ⚠️ | Fail-closed probado en código; T-1/T-6 conductuales pendientes |
| Compliance | N/A | Sin requisito formal; postura PRIV documentada |

## Plan de acción

**Fase 1 — Sesión conductual (~3h, owner humano, F-022):** runbook §Prep→A→snapshot→B→cierre; batches G1/G2/S1/S2 + F1 (`EVIDENCE_F001`, FAIL esperado); T-1 con red bloqueada primero; RESULTS con Provenance o no se consolida. Criterio: sin filas duras.
**Fase 2 — Consolidación (remota):** V→C, severidades, gate del slice (verde / verde-con-deudas / bloqueado).
**Fase 3 — Fixes (PHASE3_SPEC, alcance fijado):** fuente única + reservas + clamp + pill gratis + drain T-5 + `.no_proxy()`; witnesses invertidos mismo commit; re-run F1 debe dar PASS ≤8192; goldens re-validados.
**Roadmap:** F-008 parser (primer commit post-Phase-3) · Tier-2 GPU · soak · remote CI (F-014/F-020) · R3 · streaming/edit.

*Generado 2026-09-18 con evidencia re-ejecutada (witnesses 8+1, probe 2/2, clippy 67, secrets 0, exec-surface 3 puntos auditados). Sesión GUI pendiente — sus filas quedan 🟠 con owner, nunca 🟢.*
