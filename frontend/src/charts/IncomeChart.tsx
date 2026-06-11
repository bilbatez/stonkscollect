import { useEffect, useRef } from 'react'
import * as echarts from 'echarts/core'
import { BarChart } from 'echarts/charts'
import { GridComponent, TooltipComponent, LegendComponent } from 'echarts/components'
import { CanvasRenderer } from 'echarts/renderers'
import { bindChartResize } from './chartResize'
import { incomeChartData } from '../chartData'
import { CHART_HEIGHT } from '../constants'
import { formatCurrency } from '../format'
import type { FinancialFact, Period } from '../types'

echarts.use([BarChart, GridComponent, TooltipComponent, LegendComponent, CanvasRenderer])

export default function IncomeChart({ facts, period = 'annual' }: { facts: FinancialFact[]; period?: Period }) {
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!ref.current) return
    const chart = echarts.init(ref.current)
    const { categories, series } = incomeChartData(facts, period)
    chart.setOption({
      tooltip: { trigger: 'axis', valueFormatter: (v: number | null) => (v === null ? '—' : formatCurrency(v)) },
      legend: { bottom: 0 },
      grid: { bottom: 60 },
      xAxis: { type: 'category', data: categories },
      yAxis: { type: 'value', scale: true },
      series: series.map((s) => ({ type: 'bar', name: s.name, data: s.data })),
    })
    return bindChartResize(chart)
  }, [facts, period])

  if (incomeChartData(facts, period).categories.length === 0) return null
  return <div ref={ref} style={{ height: CHART_HEIGHT }} />
}
