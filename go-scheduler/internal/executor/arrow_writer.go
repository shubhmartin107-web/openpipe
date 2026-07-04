package executor

import (
	"bytes"
	"fmt"
	"log"
	"os"
	"os/exec"
)

type ArrowWriter struct {
	outputDir string
}

func NewArrowWriter(outputDir string) *ArrowWriter {
	return &ArrowWriter{outputDir: outputDir}
}

func (w *ArrowWriter) WriteParquet(data []byte, tableName string) error {
	path := fmt.Sprintf("%s/%s.parquet", w.outputDir, tableName)
	if err := os.MkdirAll(w.outputDir, 0755); err != nil {
		return fmt.Errorf("mkdir: %w", err)
	}
	if err := os.WriteFile(path, data, 0644); err != nil {
		return fmt.Errorf("write: %w", err)
	}
	log.Printf("Arrow data written to %s (%d bytes)", path, len(data))
	return nil
}

func (w *ArrowWriter) WriteArrowIPC(data []byte, tableName string) error {
	path := fmt.Sprintf("%s/%s.arrow", w.outputDir, tableName)
	if err := os.MkdirAll(w.outputDir, 0755); err != nil {
		return fmt.Errorf("mkdir: %w", err)
	}
	if err := os.WriteFile(path, data, 0644); err != nil {
		return fmt.Errorf("write: %w", err)
	}
	log.Printf("Arrow IPC written to %s (%d bytes)", path, len(data))
	return nil
}

func (w *ArrowWriter) WriteCSV(data []byte, tableName string) error {
	path := fmt.Sprintf("%s/%s.csv", w.outputDir, tableName)
	if err := os.MkdirAll(w.outputDir, 0755); err != nil {
		return fmt.Errorf("mkdir: %w", err)
	}
	if err := os.WriteFile(path, data, 0644); err != nil {
		return fmt.Errorf("write: %w", err)
	}
	log.Printf("CSV written to %s (%d bytes)", path, len(data))
	return nil
}

func ConvertToArrowIPC(data []byte) ([]byte, error) {
	cmd := exec.Command("python3", "-c", `
import sys, json, pyarrow as pa, pyarrow.ipc as ipc
data = json.load(sys.stdin)
arrays = []
for col in data.get("columns", []):
    arrays.append(pa.array(col["values"], type=pa.from_numpy_dtype(col.get("dtype", "int64"))))
schema = pa.schema([(col["name"], pa.from_numpy_dtype(col.get("dtype", "int64"))) for col in data.get("columns", [])])
table = pa.Table.from_arrays(arrays, schema=schema)
buf = ipc.new_stream(schema)
with ipc.new_stream(schema) as writer:
    writer.write_table(table)
sys.stdout.buffer.write(buf.to_pybytes())
	`)
	cmd.Stdin = bytes.NewReader(data)
	return cmd.Output()
}
