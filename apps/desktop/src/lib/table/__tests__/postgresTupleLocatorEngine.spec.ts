import { describe, expect, it } from "vitest";
import { postgresTupleLocatorUnsupportedEngine, shouldIncludeSyntheticRowId } from "@/lib/table/tableEditing";

const connection = (dbType: string, productName?: string, productVersion?: string) => ({
  db_type: dbType,
  database_info: productName || productVersion ? { productName, productVersion } : undefined,
});

describe("postgresTupleLocatorUnsupportedEngine (#14745)", () => {
  it("blocks Greenplum, Cloudberry, and Hologres postgres connections", () => {
    expect(postgresTupleLocatorUnsupportedEngine(connection("postgres", "Greenplum", "PostgreSQL 9.4.24 (Greenplum Database 6.18.2 build commit)"))).toBe(true);
    expect(postgresTupleLocatorUnsupportedEngine(connection("postgres", "Apache Cloudberry", "PostgreSQL 14.4 (Apache Cloudberry 1.2.0)"))).toBe(true);
    // Hologres connections reject tableoid outright (#12647).
    expect(postgresTupleLocatorUnsupportedEngine(connection("postgres", undefined, "PostgreSQL 14.x (Hologres 2.x)"))).toBe(true);
  });

  it("keeps vanilla PostgreSQL and engines without product info on the locator", () => {
    expect(postgresTupleLocatorUnsupportedEngine(connection("postgres", "PostgreSQL", "PostgreSQL 16.2 on x86_64"))).toBe(false);
    expect(postgresTupleLocatorUnsupportedEngine(connection("postgres"))).toBe(false);
    expect(postgresTupleLocatorUnsupportedEngine(undefined)).toBe(false);
    // Non-postgres engine types are decided by their own capability sets.
    expect(postgresTupleLocatorUnsupportedEngine(connection("mysql", "Greenplum", "whatever"))).toBe(false);
  });

  it("gates the synthetic row-id decision for keyless base tables (#14745)", () => {
    const keyless: string[] = [];
    expect(shouldIncludeSyntheticRowId("postgres", keyless, "TABLE")).toBe(true);
    // The engine gate is a separate predicate the injection sites combine; the
    // combination is what keeps Cloudberry AO tables from receiving the
    // tableoid/ctid/xmin projection.
    expect(shouldIncludeSyntheticRowId("postgres", keyless, "TABLE") && !postgresTupleLocatorUnsupportedEngine(connection("postgres", "Apache Cloudberry", "PostgreSQL 14.4 (Apache Cloudberry 1.2.0)"))).toBe(false);
  });
});

/**
 * OushuDB（偶数数据库）走独立 db_type，不携带 postgres-ctid synthetic key，
 * 因此无主键基表不会注入 tableoid/ctid/xmin 隐藏行号，从源头规避
 * "column xmin does not exist" 报错。
 * @author xiaogui
 * @since 2026-11-24 13:48:39
 */
describe("OushuDB never receives the PostgreSQL tuple locator", () => {
  it("does not include a synthetic row id for keyless OushuDB base tables", () => {
    expect(shouldIncludeSyntheticRowId("oushudb", [], "TABLE")).toBe(false);
    expect(shouldIncludeSyntheticRowId("oushudb", [], "BASE TABLE")).toBe(false);
  });

  it("is not treated as a postgres engine even with a PostgreSQL product name", () => {
    // OushuDB JDBC drivers report productName = PostgreSQL and a version string
    // containing Greenplum, but the independent db_type keeps it off the locator path.
    expect(postgresTupleLocatorUnsupportedEngine(connection("oushudb", "PostgreSQL", "PostgreSQL 12.12 (OushuDB 6.4.4.0) (Greenplum Database 7.0.0)"))).toBe(false);
    expect(shouldIncludeSyntheticRowId("oushudb", [], "TABLE") && !postgresTupleLocatorUnsupportedEngine(connection("oushudb", "PostgreSQL", "PostgreSQL 12.12 (OushuDB 6.4.4.0)"))).toBe(false);
  });
});
