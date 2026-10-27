@group(0) @binding(0) var<storage> prev_field: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read_write> next_field: array<vec4<f32>>;

const HEIGHT = 576;
const WIDTH = 1024;

@compute
@workgroup_size(16, 16)
fn main(
    @builtin(global_invocation_id) id: vec3<u32>
) {
    let row = id.y;
    let column = id.x;
    let prev_cell = prev_field[row * WIDTH + column];
    let above = (row + HEIGHT - 1) % HEIGHT;
    let below = (row + 1) % HEIGHT;
    let left = (column + WIDTH - 1) % WIDTH;
    let right = (column + 1) % WIDTH;

    var next_cell = 10 * prev_cell;
    next_cell += 4 * prev_field[above * WIDTH + column];
    next_cell += 4 * prev_field[below * WIDTH + column];
    next_cell += 4 * prev_field[row * WIDTH + left];
    next_cell += 4 * prev_field[row * WIDTH + right];
    next_cell += prev_field[above * WIDTH + left];
    next_cell += prev_field[above * WIDTH + right];
    next_cell += prev_field[below * WIDTH + left];
    next_cell += prev_field[below * WIDTH + right];
    next_cell /= 30;

    let xy = 0.3 * prev_cell.x * prev_cell.y * (prev_cell.x - prev_cell.y);
    let xz = 0.3 * prev_cell.x * prev_cell.z * (prev_cell.x - prev_cell.z);
    let xw = 0.3 * prev_cell.x * prev_cell.w * (prev_cell.x - prev_cell.w);
    let yz = 0.3 * prev_cell.y * prev_cell.z * (prev_cell.y - prev_cell.z);
    let yw = 0.3 * prev_cell.y * prev_cell.w * (prev_cell.y - prev_cell.w);
    let zw = 0.3 * prev_cell.z * prev_cell.w * (prev_cell.z - prev_cell.w);
    next_cell.x += xy + xz + xw;
    next_cell.y += -xy + yz + yw;
    next_cell.z += -xz - yz + zw;
    next_cell.w += -xw - yw - zw;

    next_field[row * WIDTH + column] = next_cell;
}
