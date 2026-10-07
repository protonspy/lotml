; The LLVM IR of a LotML program, written by `lotml build --target llvm`.

%lt_at = type { ptr, i32, ptr }
%lt_buf = type { ptr, i64, i64 }

@lt_text.1 = private unnamed_addr constant [1 x i8] c"\0A"
@lt_text.0 = private unnamed_addr constant [8 x i8] c"add.lot\00"

declare i32 @lt_exit(i32)
declare void @lt_buf_f64(ptr, double)
declare void @lt_buf_free(ptr)
declare void @lt_buf_put(ptr, ptr, i64)
declare void @lt_init()
declare void @lt_write(ptr, i64)

define internal double @lf_add(double %p0, double %p1) {
entry:
  %l0 = alloca double
  store double 0.0, ptr %l0
  %l1 = alloca double
  store double 0.0, ptr %l1
  %l2 = alloca double
  store double 0.0, ptr %l2
  store double %p0, ptr %l0
  store double %p1, ptr %l1
  %v1 = load double, ptr %l0
  %v2 = load double, ptr %l1
  %v3 = fadd double %v1, %v2
  store double %v3, ptr %l2
  %v4 = load double, ptr %l2
  ret double %v4
}

define internal void @lf_main() {
entry:
  %l0 = alloca double
  store double 0.0, ptr %l0
  %buf2 = alloca %lt_buf
  %v1 = call double @lf_add(double 0x3FF8000000000000, double 0x4002000000000000)
  store double %v1, ptr %l0
  store %lt_buf zeroinitializer, ptr %buf2
  %v3 = load double, ptr %l0
  call void @lt_buf_f64(ptr %buf2, double %v3)
  call void @lt_buf_put(ptr %buf2, ptr @lt_text.1, i64 1)
  %v4 = getelementptr %lt_buf, ptr %buf2, i32 0, i32 0
  %v5 = load ptr, ptr %v4
  %v6 = getelementptr %lt_buf, ptr %buf2, i32 0, i32 1
  %v7 = load i64, ptr %v6
  call void @lt_write(ptr %v5, i64 %v7)
  call void @lt_buf_free(ptr %buf2)
  ret void
}

define i32 @main() {
entry:
  call void @lt_init()
  call void @lf_main()
  %status = call i32 @lt_exit(i32 0)
  ret i32 %status
}
