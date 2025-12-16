import { getCurrentWindow } from "@tauri-apps/api/window";

export default function About() {
  const handleClose = async () => {
    await getCurrentWindow().close();
  };

  return (
    <div className="about-container">
      <div className="about-icon">
        <img src="/logo.svg" alt="Lovshot" width={64} height={64} />
      </div>
      <h1>Lovshot</h1>
      <p className="version">Version 0.1.0</p>
      <p className="description">
        A beautiful screen capture tool for macOS.
        <br />
        Screenshots, GIFs, and more.
      </p>
      <div className="about-footer">
        <p className="copyright">Made with love by LovPen</p>
        <button className="btn-primary" onClick={handleClose}>
          OK
        </button>
      </div>
    </div>
  );
}
