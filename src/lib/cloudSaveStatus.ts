import type { CloudSaveQuota, CloudSaveStatus } from '../types'

export type CloudSaveTone = 'success' | 'info' | 'warning' | 'danger' | 'neutral'

export type CloudSavePresentation = {
  title: string
  description: string
  tone: CloudSaveTone
  blocking: boolean
}

const PRESENTATIONS: Record<string, CloudSavePresentation> = {
  synced: {
    title: 'Đã đồng bộ',
    description: 'Save mới nhất đã được bảo vệ trên máy này và Google Drive.',
    tone: 'success',
    blocking: false,
  },
  ready: {
    title: 'Sẵn sàng bảo vệ',
    description: 'Launcher sẽ tự kiểm tra trước khi chơi và đồng bộ sau khi thoát game.',
    tone: 'success',
    blocking: false,
  },
  syncing: {
    title: 'Đang bảo vệ Save',
    description: 'Bạn vẫn có thể dùng launcher bình thường.',
    tone: 'info',
    blocking: false,
  },
  offline: {
    title: 'Đang chờ kết nối',
    description: 'Save mới nhất được bảo vệ trên máy này và sẽ tự đồng bộ khi có mạng.',
    tone: 'warning',
    blocking: false,
  },
  rate_limited: {
    title: 'Google Drive đang bận',
    description: 'Launcher đã lưu tác vụ và sẽ tự thử lại, không cần thao tác thủ công.',
    tone: 'warning',
    blocking: false,
  },
  storage_full: {
    title: 'Google Drive đã đầy',
    description: 'Save vẫn an toàn trên máy này nhưng chưa thể tải lên Cloud.',
    tone: 'warning',
    blocking: false,
  },
  auth_required: {
    title: 'Cần kết nối Google Drive',
    description: 'Save vẫn được bảo vệ cục bộ. Kết nối một lần để đồng bộ đa thiết bị.',
    tone: 'warning',
    blocking: false,
  },
  waiting_for_first_save: {
    title: 'Đang chờ Save đầu tiên',
    description: 'Launcher sẽ tự nhận diện ngay khi game tạo dữ liệu tiến trình.',
    tone: 'neutral',
    blocking: false,
  },
  waiting_for_save: {
    title: 'Đang chờ game ghi Save xong',
    description: 'Launcher đã giữ bản an toàn và sẽ tự đồng bộ khi file ổn định.',
    tone: 'info',
    blocking: false,
  },
  conflict_check_required: {
    title: 'Đang kiểm tra bản Save mới',
    description: 'Cloud vừa thay đổi từ thiết bị khác. Launcher không ghi đè và sẽ tự so sánh lại.',
    tone: 'warning',
    blocking: false,
  },
  permission_denied: {
    title: 'Cần kết nối lại Google Drive',
    description: 'Save trên máy vẫn an toàn; quyền Google Drive hiện không còn hợp lệ.',
    tone: 'warning',
    blocking: false,
  },
  conflict: {
    title: 'Cần chọn bản tiến trình',
    description: 'Cả bản trên máy và bản trên Cloud đều đã được giữ an toàn.',
    tone: 'danger',
    blocking: true,
  },
  remote_damaged: {
    title: 'Cloud Save cần kiểm tra',
    description: 'Dữ liệu trên máy chưa bị thay đổi. Launcher đã dừng khôi phục để bảo vệ Save.',
    tone: 'danger',
    blocking: true,
  },
  disabled: {
    title: 'Bảo vệ tự động đang tắt',
    description: 'Save chỉ được giữ trên máy này.',
    tone: 'neutral',
    blocking: false,
  },
}

export function cloudSavePresentation(status: Pick<CloudSaveStatus, 'state' | 'lastMessage'> | null): CloudSavePresentation {
  const state = status?.state || 'disabled'
  return PRESENTATIONS[state] ?? {
    title: 'Cloud Save',
    description: status?.lastMessage || 'Launcher đang kiểm tra trạng thái Save.',
    tone: 'neutral',
    blocking: false,
  }
}

export function quotaPercent(quota: Pick<CloudSaveQuota, 'limitBytes' | 'usageBytes'> | null): number | null {
  if (!quota || quota.limitBytes == null || !Number.isFinite(quota.limitBytes) || quota.limitBytes <= 0) return null
  return Math.max(0, Math.min(100, Math.round((quota.usageBytes / quota.limitBytes) * 100)))
}

export function pendingSummary(status: Pick<CloudSaveStatus, 'pendingOperationCount' | 'pendingUploadBytes'> | null) {
  if (!status?.pendingOperationCount) return null
  return { count: status.pendingOperationCount, bytes: status.pendingUploadBytes }
}
