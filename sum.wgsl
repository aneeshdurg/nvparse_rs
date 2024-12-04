struct Vector {
    data: [[stride(4)]] array<u32>;
};

[[group(0), binding(0)]] var<storage, read>  a: Vector;
[[group(0), binding(1)]] var<storage, read>  chunk_size: Vector;
[[group(0), binding(2)]] var<storage, read>  data_len: Vector;
[[group(0), binding(3)]] var<storage, read_write> c: Vector;

[[stage(compute), workgroup_size(1)]]
fn main([[builtin(global_invocation_id)]] global_id: vec3<u32>) {
  c.data[global_id.x] = 0u;
  var start: u32 = global_id.x * chunk_size.data[0];
  var nelems: u32 = min(chunk_size.data[0], data_len.data[0] - start);
  for (var i: u32 = 0u; i < nelems; i = i + 1u) {
    c.data[global_id.x] = c.data[global_id.x] + a.data[start + i];
  }
}
