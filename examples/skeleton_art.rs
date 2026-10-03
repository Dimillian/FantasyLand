use fantasy_land::people_sprites::{creature_preview,COMBAT_W,COMBAT_H,COMBAT_FRAMES};
fn main(){
    let width=COMBAT_W*COMBAT_FRAMES as usize;let height=COMBAT_H*3;
    let mut sheet=vec![0u8;width*height*4];
    for kind in 0..3 {for pose in 0..COMBAT_FRAMES {
        let sprite=creature_preview(kind,0,pose);
        for y in 0..COMBAT_H {
            let dst=((kind as usize*COMBAT_H+y)*width+pose as usize*COMBAT_W)*4;
            sheet[dst..dst+COMBAT_W*4].copy_from_slice(&sprite[y*COMBAT_W*4..(y+1)*COMBAT_W*4]);
        }
    }}
    let path=std::env::args().nth(1).unwrap_or("dist/foes-study.png".into());
    let file=std::io::BufWriter::new(std::fs::File::create(&path).unwrap());
    let mut encoder=png::Encoder::new(file,width as u32,height as u32);encoder.set_color(png::ColorType::Rgba);encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(&sheet).unwrap();println!("{path}: 24 frames × 3 creatures, 96×192 each");
}
