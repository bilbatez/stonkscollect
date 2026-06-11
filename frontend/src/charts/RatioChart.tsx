import { useEffect, useRef } from 'react'
import * as echarts from 'echarts/core'
import { LineChart } from 'echarts/charts'
import { GridComponent, TooltipComponent, LegendComponent } from 'echarts/components'
import { CanvasRenderer } from 'echarts/renderers'
import { bindChartResize } from './chartResize'
import { ratioChartData } from '../chartData'
import { CHART_HEIGHT } from '../constants'
import type { Period, Ratio } from '../types'

echarts.use([LineChart, GridComponent, TooltipComponent, LegendComponent, CanvasRenderer])

export default function RatioChart({ ratios, period = 'annual' }: { ratios: Ratio[]; period?: Period }) {
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!ref.current) return
    const chart = echarts.init(ref.current)
    const { categories, series } = ratioChartData(ratios, period)
    chart.setOption({
      tooltip: { trigger: 'axis' },
      legend: { bottom: 0 },
      grid: { bottom: 60 },
      xAxis: { type: 'category', data: categories },
      yAxis: { type: 'value', scale: true },
      series: series.map((s) => ({ type: 'line', name: s.name, data: s.data, connectNulls: false })),
    })
    return bindChartResize(chart)
  }, [ratios, period])

  if (ratioChartData(ratios, period).categories.length === 0) return null
  return <div ref={ref} style={{ height: CHART_HEIGHT }} />
}
