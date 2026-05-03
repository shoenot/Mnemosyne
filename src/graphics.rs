use limine::framebuffer::Framebuffer;

pub fn putpixel(x: u32, y: u32, color: u32, fb: &Framebuffer) -> Option<u32> {
    let pixels_per_row = fb.pitch / 4;
    let ptr = fb.address().cast::<u32>();
    
    if x >= fb.width as u32 || y >= fb.height as u32 { return None };

    unsafe {
        ptr.add((y * pixels_per_row as u32 + x) as usize).write_volatile(color);
    }
    Some(color)
}

pub fn draw_diagonal(fb: &Framebuffer) {
    for i in 0..fb.height {
        putpixel(i as u32, i as u32, 0xFF0000, fb);
    }
}
