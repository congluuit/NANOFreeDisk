+++
id = "ollama"
risk = "caution"
paths = ["${HOME}/.ollama"]
rules = ["ollama.models"]
+++

## en
### Ollama models

Local AI models downloaded by Ollama (`models\blobs` holds the weights, often 4–40 GB each) plus its keys and history.

**If you delete it:** every model must be downloaded again and your Ollama identity key is regenerated. Nothing breaks permanently.

**Instead:** list models with `ollama list` and remove the ones you no longer use with `ollama rm <name>`; set `OLLAMA_MODELS` to keep models on a bigger drive.

## vi
### Mô hình Ollama

Các mô hình AI chạy cục bộ mà Ollama đã tải (`models\blobs` chứa trọng số, thường 4–40 GB mỗi mô hình) cùng khóa và lịch sử của nó.

**Nếu xóa:** phải tải lại mọi mô hình và khóa định danh Ollama được tạo lại. Không hỏng gì vĩnh viễn.

**Nên làm:** xem danh sách bằng `ollama list` và xóa mô hình không dùng bằng `ollama rm <tên>`; đặt `OLLAMA_MODELS` để lưu mô hình sang ổ lớn hơn.
