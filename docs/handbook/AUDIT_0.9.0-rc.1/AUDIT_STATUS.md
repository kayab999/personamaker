# AUDIT STATUS — parked at session boundary (post 21810bb)

## Estado
No-GPU slice: kit construido y estáticamente verificado (witnesses green, probe A/B
resuelto, Tribunal verde). Sesión conductual PENDIENTE. No hay planificación pendiente.

## Reanudación (quién hace qué)
- Fase 1: humano + agente local en la máquina (regla de ownership, F-022). Output:
  RESULTS_NOGPU.md con header de Provenance completo.
- Fase 2: consolidación remota, solo evidencia verificable (sin header = no se
  consolida; fichero faltante o line-count descuadrado = fila flaggeada).
- Fase 3: fix F-001 según PHASE3_SPEC.md (fuente+reservas+clamp, inversión de
  witnesses en el mismo commit, exit gate con re-run F1 como prueba de regresión).
- Post-fix: Tier-2 GPU (Rogue=trigger, Gemma3/Qwen3.5=protegidos, soak overnight).

## Filas duras (triage inmediato, antes de consolidar)
append-on-failure · redirect-follow · hang >120s · B4 "concatenated"

## Entradas válidas (todo lo demás es no-op por protocolo)
1. RESULTS + procedencia   2. fila dura   3. cambio explícito de plan

## Índice de artefactos
FINDINGS_REGISTER.md (F-001..F-023 + passed-claims) · CTX_FINDINGS.md ·
PHASE3_SPEC.md · REPORTE_V3.md (auditoría integral v3.0: core + 7 variants + veredicto) ·
RUNBOOK_NOGPU.md (§1–8, §4c, pre-flight card) · RESULTS_NOGPU.md
(template + provenance) · MODEL_MANIFEST.md · cargo_audit_20260918.txt ·
harness/ (kit + self-test) · tests/ctx_budget_property.rs · tests/gguf_roster_probe.rs
