"use client";

import DataTable from "@/components/viz/DataTable";
import { adaptDataTableData } from "@/lib/villa/adapters/data-table";

interface DataTableWidgetProps {
  data: unknown;
  dimensions: { width: number; height: number };
}

export default function DataTableWidget({ data, dimensions }: DataTableWidgetProps) {
  const { columns, rows } = adaptDataTableData(data);

  if (rows.length === 0) {
    return (
      <div className="flex items-center justify-center h-full">
        <span className="text-xs text-slate-600 font-mono">No data</span>
      </div>
    );
  }

  return (
    <div
      style={{ width: dimensions.width, height: dimensions.height }}
      className="overflow-hidden"
    >
      <DataTable data={rows} columns={columns} sortable />
    </div>
  );
}
