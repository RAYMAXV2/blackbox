<script setup lang="ts">
import { computed } from 'vue'
import Led from '../components/Led.vue'
import DotMatrix from '../components/DotMatrix.vue'
import { now, system } from '../store'
import { has, t } from '../i18n'
import { fmtDateTime, fmtDuration, fmtMb, fmtNum } from '../lib/format'
import type { HealthCheck } from '../lib/contract'

const s = computed(() => system.value)

/** Checks with a `reason` param (e.g. chk.cpu_temp) get one string per reason; others one per state. */
function checkKey(c: HealthCheck): string {
  const reason = c.params.reason
  if (typeof reason === 'string' && has(`${c.id}.${reason}`)) return `${c.id}.${reason}`
  return `${c.id}.${c.state}`
}
const rows = computed(() => {
  const x = s.value
  if (!x) return []
  return [
    { k: 'system.os', v: x.os },
    { k: 'system.build', v: x.build },
    { k: 'system.cpu', v: x.cpu },
    { k: 'system.cores', v: `${fmtNum(x.cores)} / ${fmtNum(x.threads)}` },
    { k: 'system.gpu', v: x.gpu ?? t('common.na') },
    { k: 'system.ram', v: fmtMb(x.ramMb, 0) },
    { k: 'system.elevated', v: x.elevated ? t('common.yes') : t('common.no') },
    { k: 'system.boot', v: `${fmtDateTime(x.bootTs)} · ${fmtDuration((now.value - x.bootTs) / 1000)}` },
    { k: 'system.version', v: x.version },
  ]
})
</script>

<template>
  <div class="page">
    <header class="page-head">
      <div>
        <h1>{{ t('system.title') }}</h1>
        <p>{{ t('system.sub') }}</p>
      </div>
    </header>

    <div v-if="s" class="grid">
      <section class="card sys-info">
        <div class="os">
          <DotMatrix :text="s.os.replace(/^Windows\s*/i, 'WIN ')" :dot="3.2" />
        </div>
        <dl>
          <div v-for="r in rows" :key="r.k" class="row">
            <dt class="label">{{ t(r.k) }}</dt>
            <dd>{{ r.v }}</dd>
          </div>
        </dl>
      </section>

      <section class="card sys-checks">
        <div class="head"><span class="label">{{ t('system.checks') }}</span></div>
        <ul>
          <li v-for="c in s.checks" :key="c.id">
            <Led :state="c.state === 'ok' ? 'ok' : c.state === 'warn' ? 'warn' : 'info'" :size="8" />
            <div>
              <div class="ttl">{{ t(c.id) }}</div>
              <div class="msg">{{ t(checkKey(c), c.params) }}</div>
            </div>
          </li>
        </ul>
      </section>
    </div>
  </div>
</template>

<style scoped>
h1 {
  margin: 0;
  font-size: 26px;
  font-weight: 600;
  letter-spacing: -0.02em;
}
.grid {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  gap: 16px;
  align-items: start;
}
@media (max-width: 1100px) {
  .grid {
    grid-template-columns: 1fr;
  }
}
.sys-info,
.sys-checks {
  padding: 20px 22px;
}
.os {
  padding-bottom: 18px;
  border-bottom: 1px solid var(--line);
  margin-bottom: 8px;
  overflow: hidden;
}
dl {
  margin: 0;
}
.row {
  display: grid;
  grid-template-columns: 150px minmax(0, 1fr);
  gap: 16px;
  padding: 10px 0;
  border-bottom: 1px solid var(--line);
}
.row:last-child {
  border-bottom: 0;
}
dd {
  margin: 0;
  font-size: 13px;
  user-select: text;
}
.head {
  margin-bottom: 8px;
}
ul {
  list-style: none;
  margin: 0;
  padding: 0;
}
li {
  display: flex;
  gap: 14px;
  align-items: flex-start;
  padding: 12px 0;
  border-bottom: 1px solid var(--line);
}
li:last-child {
  border-bottom: 0;
}
li .led {
  margin-top: 6px;
}
.ttl {
  font-size: 13.5px;
}
.msg {
  font-size: 12.5px;
  color: var(--text-2);
}
</style>
