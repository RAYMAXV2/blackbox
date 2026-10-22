<script setup lang="ts">
import { computed } from 'vue'
import Meter from '../Meter.vue'
import Spark from '../Spark.vue'
import { history, status } from '../../store'
import { t } from '../../i18n'
import { fmtMb, fmtMs, fmtNum, fmtPct, fmtRate } from '../../lib/format'

const s = computed(() => status.value)
const cpu = computed(() => s.value?.cpu ?? null)
const mem = computed(() => s.value?.memory ?? null)
const gpu = computed(() => s.value?.gpu ?? null)
const disks = computed(() => s.value?.disks ?? [])
const net = computed(() => s.value?.net ?? null)
const latTone = (ms: number) => (ms >= 20 ? 'bad' : ms >= 5 ? 'warn' : '')
</script>

<template>
  <div class="hw">
    <!-- CPU -->
    <article v-if="cpu" class="card box cpu">
      <div class="box-head">
        <span class="label">{{ t('hw.cpu') }}</span>
        <span class="chip" :class="cpu.throttled ? 'red' : ''">
          {{ cpu.throttled ? t('hw.throttled') : `${t('hw.freqLimit')} ${fmtPct(cpu.freqLimitPct)}` }}
        </span>
      </div>
      <div class="model">{{ cpu.name }}</div>
      <div class="big-row">
        <b class="big num">{{ fmtPct(cpu.totalPct) }}</b>
        <Spark class="spark" :values="history.cpu" tone="kernel" />
      </div>
      <div class="cores" :style="{ gridTemplateColumns: `repeat(${cpu.perCore.length}, 1fr)` }">
        <div v-for="(c, i) in cpu.perCore" :key="i" class="core" :title="`#${i} · ${fmtPct(c)}`">
          <div class="core-fill" :style="{ height: `${Math.max(2, c)}%` }" :class="{ hot: c > 90 }" />
        </div>
      </div>
      <dl class="kv">
        <div><dt>{{ t('hw.kernel') }}</dt><dd class="num">{{ fmtPct(cpu.kernelPct, 1) }}</dd></div>
        <div><dt>{{ t('hw.processes') }}</dt><dd class="num">{{ fmtNum(cpu.processes) }}</dd></div>
        <div><dt>{{ t('hw.threads') }}</dt><dd class="num">{{ fmtNum(cpu.threads) }}</dd></div>
      </dl>
    </article>

    <!-- GPU -->
    <article class="card box gpu">
      <div class="box-head">
        <span class="label">{{ t('hw.gpu') }}</span>
        <template v-if="gpu">
          <span v-if="!gpu.throttle.length" class="chip green">{{ t('hw.noThrottle') }}</span>
          <span v-for="r in gpu.throttle" v-else :key="r" class="chip" :class="r === 'gpu_idle' ? '' : 'amber'">{{ t('gpuThrottle.' + r) }}</span>
        </template>
      </div>
      <template v-if="gpu">
        <div class="model">{{ gpu.name }}</div>
        <div class="big-row">
          <b class="big num">{{ fmtPct(gpu.utilPct) }}</b>
          <Spark class="spark" :values="history.gpu" tone="hardware" />
        </div>
        <dl class="kv grid3">
          <div><dt>{{ t('hw.temp') }}</dt><dd class="num" :class="{ warn: (gpu.tempC ?? 0) >= 83 }">{{ gpu.tempC !== null ? `${fmtNum(gpu.tempC)} °C` : '—' }}</dd></div>
          <div><dt>{{ t('hw.power') }}</dt><dd class="num">{{ gpu.powerW !== null ? `${fmtNum(gpu.powerW)} / ${fmtNum(gpu.powerLimitW)} W` : '—' }}</dd></div>
          <div><dt>{{ t('hw.clocks') }}</dt><dd class="num">{{ gpu.coreMhz !== null ? `${fmtNum(gpu.coreMhz)} MHz` : '—' }}</dd></div>
          <div><dt>{{ t('hw.memUtil') }}</dt><dd class="num">{{ fmtPct(gpu.memUtilPct) }}</dd></div>
          <div><dt>{{ t('hw.fan') }}</dt><dd class="num">{{ gpu.fanPct !== null ? fmtPct(gpu.fanPct) : '—' }}</dd></div>
          <div><dt>{{ t('hw.vram') }}</dt><dd class="num">{{ fmtMb(gpu.vramUsedMb) }}</dd></div>
        </dl>
        <Meter :value="(gpu.vramUsedMb / Math.max(1, gpu.vramTotalMb)) * 100" tone="auto" :warn-at="85" :bad-at="95" />
        <div class="foot mono">{{ t('hw.vram') }} · {{ fmtMb(gpu.vramUsedMb) }} / {{ fmtMb(gpu.vramTotalMb) }}</div>
      </template>
      <p v-else class="none">{{ t('hw.noGpu') }}</p>
    </article>

    <!-- Memory -->
    <article v-if="mem" class="card box mem">
      <div class="box-head">
        <span class="label">{{ t('hw.memory') }}</span>
        <span class="num dim small">{{ fmtMb(mem.totalMb) }}</span>
      </div>
      <div class="big-row">
        <b class="big num">{{ fmtPct(mem.loadPct) }}</b>
        <Spark class="spark" :values="history.mem" tone="apps" />
      </div>
      <Meter :value="mem.loadPct" tone="auto" :warn-at="85" :bad-at="94" />
      <dl class="kv">
        <div><dt>{{ t('hw.used') }}</dt><dd class="num">{{ fmtMb(mem.usedMb) }}</dd></div>
        <div><dt>{{ t('hw.available') }}</dt><dd class="num">{{ fmtMb(mem.availableMb) }}</dd></div>
        <div><dt>{{ t('kernel.hardFaults') }}</dt><dd class="num">{{ fmtNum(mem.hardFaultsPerSec) }}</dd></div>
      </dl>
      <div class="sub-meter">
        <div class="foot mono">{{ t('hw.commit') }} · {{ fmtMb(mem.commitMb) }} / {{ fmtMb(mem.commitLimitMb) }}</div>
        <Meter :value="(mem.commitMb / Math.max(1, mem.commitLimitMb)) * 100" tone="auto" :warn-at="85" :bad-at="95" :height="3" />
      </div>
    </article>

    <!-- Disks + network -->
    <article class="card box io">
      <div class="box-head">
        <span class="label">{{ t('hw.disks') }}</span>
      </div>
      <div v-if="disks.length" class="disks">
        <div v-for="d in disks" :key="d.id" class="disk">
          <div class="disk-name mono">{{ d.label }}</div>
          <div class="disk-cols">
            <span><em>{{ t('hw.read') }}</em><b class="num">{{ fmtRate(d.readBps) }}</b></span>
            <span><em>{{ t('hw.write') }}</em><b class="num">{{ fmtRate(d.writeBps) }}</b></span>
            <span><em>{{ t('hw.iops') }}</em><b class="num">{{ fmtNum(d.iops) }}</b></span>
            <span>
              <em>{{ t('hw.latency') }}</em>
              <b class="num" :class="latTone(d.latAvgMs)">{{ fmtMs(d.latAvgMs, 2) }}</b>
              <i class="num" :class="latTone(d.latMaxMs)">{{ t('hw.latMax') }} {{ fmtMs(d.latMaxMs) }}</i>
            </span>
          </div>
        </div>
      </div>
      <p v-else class="none">{{ t('hw.noDisk') }}</p>
      <div v-if="net" class="net">
        <span class="label">{{ t('hw.network') }}</span>
        <span class="num"><em>↓</em> {{ fmtRate(net.rxBps) }}</span>
        <span class="num"><em>↑</em> {{ fmtRate(net.txBps) }}</span>
      </div>
    </article>
  </div>
</template>

<style scoped>
.hw {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}
@media (min-width: 1500px) {
  .hw {
    grid-template-columns: repeat(4, minmax(0, 1fr));
  }
}
.box {
  padding: 16px 18px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  border-radius: 16px;
  min-width: 0;
}
.box-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.model {
  font-size: 12px;
  color: var(--text-2);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.big-row {
  display: flex;
  align-items: flex-end;
  gap: 14px;
}
.big {
  font-size: 30px;
  font-weight: 500;
  letter-spacing: -0.03em;
  line-height: 1;
  min-width: 82px;
}
.spark {
  flex: 1;
}
.cores {
  display: grid;
  gap: 3px;
  height: 46px;
}
.core {
  position: relative;
  background: var(--surface-3);
  border-radius: 3px;
  overflow: hidden;
}
.core-fill {
  position: absolute;
  inset: auto 0 0 0;
  background: var(--l-kernel);
  border-radius: 3px;
  transition: height 0.5s var(--ease);
}
.core-fill.hot {
  background: var(--red);
}
.kv {
  margin: 0;
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px 12px;
}
.kv div {
  min-width: 0;
}
.kv dt {
  font-family: var(--font-mono);
  font-size: 9.5px;
  letter-spacing: 0.1em;
  text-transform: uppercase;
  color: var(--text-3);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.kv dd {
  margin: 2px 0 0;
  font-size: 13px;
  white-space: nowrap;
}
.kv dd.warn {
  color: var(--amber);
}
.foot {
  font-size: 10.5px;
  color: var(--text-3);
}
.sub-meter {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 2px;
}
.small {
  font-size: 11px;
}
.none {
  margin: 0;
  font-size: 12.5px;
  color: var(--text-3);
}
.disks {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.disk {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding-bottom: 10px;
  border-bottom: 1px solid var(--line);
}
.disk-name {
  font-size: 12px;
}
.disk-cols {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 8px;
}
.disk-cols span {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}
.disk-cols em,
.net em {
  font-style: normal;
  font-family: var(--font-mono);
  font-size: 9.5px;
  letter-spacing: 0.1em;
  text-transform: uppercase;
  color: var(--text-3);
}
.disk-cols b {
  font-weight: 400;
  font-size: 12.5px;
  white-space: nowrap;
}
.disk-cols i {
  font-style: normal;
  font-size: 10.5px;
  color: var(--text-3);
}
.warn {
  color: var(--amber) !important;
}
.bad {
  color: var(--red) !important;
}
.net {
  margin-top: auto;
  display: flex;
  align-items: center;
  gap: 16px;
  font-size: 12.5px;
}
.net em {
  font-size: 12px;
  margin-right: 2px;
}
</style>
