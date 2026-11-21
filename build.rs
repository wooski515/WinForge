//! Вшивает иконку (assets/winforge.ico) в winforge.exe: её показывают Проводник,
//! ярлыки, панель задач и «Пуск».

fn main() {
    println!("cargo:rerun-if-changed=assets/winforge.rc");
    println!("cargo:rerun-if-changed=assets/winforge.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/winforge.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("не удалось вшить иконку в exe (нужен rc.exe из Windows SDK)");
    }
}
