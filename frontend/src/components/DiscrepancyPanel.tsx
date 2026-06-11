import { formatCurrency } from '../format'
import type { Discrepancy } from '../types'
import { DataGrid } from './DataGrid'
import type { GridColumn } from './dataGridUtils'

/** Cross-source mismatches flagged by the reconcile layer (sortable / filterable
 *  / column-reorderable grid). */
export function DiscrepancyPanel({ discrepancies }: { discrepancies: Discrepancy[] }) {
  const columns: GridColumn<Discrepancy>[] = [
    { id: 'field', header: 'Field', sortValue: (d) => d.field, filter: true, cell: (d) => d.field },
    { id: 'period', header: 'Period', sortValue: (d) => d.period ?? '', cell: (d) => d.period ?? '—' },
    {
      id: 'sources',
      header: 'Sources',
      cell: (d) =>
        `${d.source_a}: ${formatCurrency(d.value_a)} vs ${d.source_b}: ${formatCurrency(d.value_b)}`,
    },
    {
      id: 'diff',
      header: 'Diff',
      sortValue: (d) => d.pct_diff,
      cell: (d) => `${(d.pct_diff * 100).toFixed(1)}%`,
    },
  ]
  return (
    <DataGrid
      columns={columns}
      rows={discrepancies}
      getRowId={(d) => `${d.field}-${d.period ?? 'na'}-${d.source_a}-${d.value_a}-${d.value_b}`}
      empty="No discrepancies flagged."
    />
  )
}
