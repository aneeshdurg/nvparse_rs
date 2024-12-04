struct Vector32 {
    data: [[stride(4)]] array<u32>;
};

[[group(0), binding(0)]] var<storage, read>  a: Vector32;
[[group(0), binding(1)]] var<storage, read>  chunk_size: Vector32;
[[group(0), binding(2)]] var<storage, read>  data_len: Vector32;
[[group(0), binding(3)]] var<storage, read>  char: Vector32;
[[group(0), binding(4)]] var<storage, read_write> c: Vector32;

[[stage(compute), workgroup_size(1)]]
fn main([[builtin(global_invocation_id)]] global_id: vec3<u32>) {
    c.data[global_id.x] = 0u;

    var start: u32 = global_id.x * chunk_size.data[0];
    var nelems: u32 = min(chunk_size.data[0], data_len.data[0] - start);

    var rem: u32 = start % 4u;
    if (rem != 0u) {
        var src: u32 = a.data[start / 4u];

        // unpack a u32 into 4 x u8
        var c0: u32 = src & 255u;
        src = src / 256u;
        var c1: u32 = src & 255u;
        src = src / 256u;
        var c2: u32 = src & 255u;
        src = src / 256u;
        var c3: u32 = src & 255u;

        if (rem <= 0u && c0 == char.data[0]) {
            c.data[global_id.x] = c.data[global_id.x] + 1u;
        }
        if (rem <= 1u && c1 == char.data[0]) {
            c.data[global_id.x] = c.data[global_id.x] + 1u;
        }
        if (rem <= 2u && c2 == char.data[0]) {
            c.data[global_id.x] = c.data[global_id.x] + 1u;
        }
        if (rem <= 3u && c3 == char.data[0]) {
            c.data[global_id.x] = c.data[global_id.x] + 1u;
        }

        start = start + 4u - rem;
        nelems = nelems - (4u - rem);
    }

    for (var i: u32 = 0u; i < nelems; i = i + 4u) {
        var src: u32 = a.data[(start + i) / 4u];

        var c0: u32 = src & 255u;
        src = src / 256u;
        var c1: u32 = src & 255u;
        src = src / 256u;
        var c2: u32 = src & 255u;
        src = src / 256u;
        var c3: u32 = src & 255u;

        if ((i + 0u) < nelems && c0 == char.data[0]) {
            c.data[global_id.x] = c.data[global_id.x] + 1u;
        }
        if ((i + 1u) < nelems && c1 == char.data[0]) {
            c.data[global_id.x] = c.data[global_id.x] + 1u;
        }
        if ((i + 2u) < nelems && c2 == char.data[0]) {
            c.data[global_id.x] = c.data[global_id.x] + 1u;
        }
        if ((i + 3u) < nelems && c3 == char.data[0]) {
            c.data[global_id.x] = c.data[global_id.x] + 1u;
        }
    }
}
