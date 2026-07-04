package executor

import (
	"context"
	"fmt"
	"log"
	"os/exec"
	"strings"
)

type SQLLakehouseExecutor struct {
	driver      string
	connStr     string
	arrowWriter *ArrowWriter
}

func NewSQLLakehouseExecutor(driver, connStr string) *SQLLakehouseExecutor {
	return &SQLLakehouseExecutor{
		driver:      driver,
		connStr:     connStr,
		arrowWriter: NewArrowWriter(".openpipe/output"),
	}
}

func (e *SQLLakehouseExecutor) Execute(ctx context.Context, model CompiledModel) error {
	sql := model.CompiledSQL
	if sql == "" {
		return fmt.Errorf("empty compiled SQL for model '%s'", model.Name)
	}

	log.Printf("Executing model '%s' as %s: %s", model.Name, model.Materialization, model.RelationName)

	switch model.Materialization {
	case "view":
		return e.executeView(ctx, model)
	case "table":
		return e.executeTable(ctx, model)
	case "incremental":
		return e.executeIncremental(ctx, model)
	case "ephemeral":
		log.Printf("Ephemeral model '%s' skipped (used only as CTE)", model.Name)
		return nil
	default:
		return e.executeView(ctx, model)
	}
}

func (e *SQLLakehouseExecutor) executeView(ctx context.Context, model CompiledModel) error {
	sql := fmt.Sprintf("CREATE OR REPLACE VIEW %s AS %s", model.RelationName, model.CompiledSQL)
	return e.runSQL(ctx, sql, model.Name)
}

func (e *SQLLakehouseExecutor) executeTable(ctx context.Context, model CompiledModel) error {
	sql := fmt.Sprintf("CREATE OR REPLACE TABLE %s AS %s", model.RelationName, model.CompiledSQL)
	return e.runSQL(ctx, sql, model.Name)
}

func (e *SQLLakehouseExecutor) executeIncremental(ctx context.Context, model CompiledModel) error {
	tableExists := e.checkTableExists(ctx, model.RelationName)

	if !tableExists {
		sql := fmt.Sprintf("CREATE TABLE %s AS %s", model.RelationName, model.CompiledSQL)
		return e.runSQL(ctx, sql, model.Name)
	}

	uniqueKey := ""
	if uk, ok := model.Config["unique_key"]; ok {
		uniqueKey = fmt.Sprintf("%v", uk)
	}

	if uniqueKey != "" {
		sql := fmt.Sprintf(
			`MERGE INTO %s AS target USING (%s) AS source ON target.%s = source.%s
			 WHEN MATCHED THEN UPDATE SET * WHEN NOT MATCHED THEN INSERT *`,
			model.RelationName, model.CompiledSQL, uniqueKey, uniqueKey,
		)
		return e.runSQL(ctx, sql, model.Name)
	}

	sql := fmt.Sprintf("INSERT INTO %s %s", model.RelationName, model.CompiledSQL)
	return e.runSQL(ctx, sql, model.Name)
}

func (e *SQLLakehouseExecutor) checkTableExists(ctx context.Context, tableName string) bool {
	sql := fmt.Sprintf("SELECT 1 FROM %s LIMIT 1", tableName)
	err := e.runSQL(ctx, sql, "__check_table_exists")
	return err == nil
}

func (e *SQLLakehouseExecutor) runSQL(ctx context.Context, sql string, modelName string) error {
	log.Printf("SQL [%s]: %s", modelName, truncateSQL(sql, 200))

	var data []byte
	var err error

	switch e.driver {
	case "spark":
		data, err = e.runSparkSQL(ctx, sql)
	case "trino":
		data, err = e.runTrinoSQL(ctx, sql)
	case "duckdb":
		data, err = e.runDuckDBSQL(ctx, sql)
	case "stdout":
		log.Printf("[stdout executor] Would execute: %s", sql)
		e.arrowWriter.WriteCSV(nil, modelName)
		return nil
	default:
		data, err = e.runSparkSQL(ctx, sql)
	}

	if err != nil {
		return err
	}

	if e.arrowWriter != nil && data != nil {
		if parquetErr := e.arrowWriter.WriteParquet(data, modelName); parquetErr != nil {
			log.Printf("Arrow write warning for %s: %v", modelName, parquetErr)
		}
	}

	return nil
}

func (e *SQLLakehouseExecutor) runSparkSQL(ctx context.Context, sql string) ([]byte, error) {
	cmd := exec.CommandContext(ctx, "spark-sql", "-e", sql)
	output, err := cmd.CombinedOutput()
	if err != nil {
		return output, fmt.Errorf("spark-sql error: %w\nOutput: %s", err, string(output))
	}
	return output, nil
}

func (e *SQLLakehouseExecutor) runTrinoSQL(ctx context.Context, sql string) ([]byte, error) {
	cmd := exec.CommandContext(ctx, "trino", "--execute", sql)
	if e.connStr != "" {
		cmd.Args = append(cmd.Args, "--server", e.connStr)
	}
	output, err := cmd.CombinedOutput()
	if err != nil {
		return output, fmt.Errorf("trino error: %w\nOutput: %s", err, string(output))
	}
	return output, nil
}

func (e *SQLLakehouseExecutor) runDuckDBSQL(ctx context.Context, sql string) ([]byte, error) {
	cmd := exec.CommandContext(ctx, "duckdb", "-c", sql)
	if e.connStr != "" {
		cmd.Args = append(cmd.Args, e.connStr)
	}
	output, err := cmd.CombinedOutput()
	if err != nil {
		return output, fmt.Errorf("duckdb error: %w\nOutput: %s", err, string(output))
	}
	return output, nil
}

func truncateSQL(sql string, maxLen int) string {
	singleLine := strings.ReplaceAll(sql, "\n", " ")
	if len(singleLine) > maxLen {
		return singleLine[:maxLen] + "..."
	}
	return singleLine
}
