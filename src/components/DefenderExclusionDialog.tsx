import { motion } from 'motion/react';
import { Shield, Check, X, AlertTriangle } from 'lucide-react';
import { useState } from 'react';

interface DefenderExclusionDialogProps {
  isOpen: boolean;
  onClose: () => void;
  onAccept: () => Promise<void>;
  path: string;
}

export function DefenderExclusionDialog({
  isOpen,
  onClose,
  onAccept,
  path,
}: DefenderExclusionDialogProps) {
  const [isProcessing, setIsProcessing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!isOpen) return null;

  const handleAccept = async () => {
    setIsProcessing(true);
    setError(null);
    try {
      await onAccept();
      onClose();
    } catch (err: any) {
      setError(err?.message || 'Failed to add exclusion');
    } finally {
      setIsProcessing(false);
    }
  };

  return (
    <motion.div
      className="fixed inset-0 z-[9999] flex items-center justify-center bg-black/60 backdrop-blur-sm"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      onClick={onClose}
    >
      <motion.div
        className="relative w-full max-w-md rounded-2xl bg-gradient-to-br from-gray-900/95 to-gray-800/95 p-8 shadow-2xl border border-gray-700/50"
        initial={{ scale: 0.9, opacity: 0 }}
        animate={{ scale: 1, opacity: 1 }}
        exit={{ scale: 0.9, opacity: 0 }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center gap-3 mb-6">
          <div className="flex-shrink-0 w-12 h-12 rounded-xl bg-blue-500/20 flex items-center justify-center">
            <Shield className="w-6 h-6 text-blue-400" />
          </div>
          <div>
            <h2 className="text-xl font-bold text-white">
              Improve Download Performance
            </h2>
            <p className="text-sm text-gray-400">One-time setup</p>
          </div>
        </div>

        {/* Description */}
        <div className="mb-6 space-y-3">
          <p className="text-gray-300 leading-relaxed">
            Add <span className="font-mono text-blue-400">{path}</span> to
            Windows Defender exclusions to prevent download interruptions and
            I/O errors.
          </p>

          {/* Benefits */}
          <div className="space-y-2 mt-4">
            <div className="flex items-center gap-2 text-sm text-gray-300">
              <Check className="w-4 h-4 text-green-400" />
              <span>Faster and more reliable downloads</span>
            </div>
            <div className="flex items-center gap-2 text-sm text-gray-300">
              <Check className="w-4 h-4 text-green-400" />
              <span>No file access conflicts</span>
            </div>
            <div className="flex items-center gap-2 text-sm text-gray-300">
              <Check className="w-4 h-4 text-green-400" />
              <span>Set once, works forever</span>
            </div>
          </div>

          {/* Warning */}
          <div className="mt-4 p-3 rounded-lg bg-yellow-500/10 border border-yellow-500/30">
            <div className="flex gap-2">
              <AlertTriangle className="w-4 h-4 text-yellow-400 flex-shrink-0 mt-0.5" />
              <p className="text-xs text-yellow-200">
                This requires administrator permission. Windows UAC prompt will
                appear.
              </p>
            </div>
          </div>
        </div>

        {/* Error Message */}
        {error && (
          <div className="mb-4 p-3 rounded-lg bg-red-500/10 border border-red-500/30">
            <div className="flex gap-2">
              <X className="w-4 h-4 text-red-400 flex-shrink-0 mt-0.5" />
              <p className="text-xs text-red-200">{error}</p>
            </div>
          </div>
        )}

        {/* Actions */}
        <div className="flex gap-3">
          <button
            onClick={onClose}
            disabled={isProcessing}
            className="flex-1 px-4 py-2.5 rounded-lg bg-gray-700/50 hover:bg-gray-700 text-white font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Skip for Now
          </button>
          <button
            onClick={handleAccept}
            disabled={isProcessing}
            className="flex-1 px-4 py-2.5 rounded-lg bg-gradient-to-r from-blue-500 to-blue-600 hover:from-blue-600 hover:to-blue-700 text-white font-medium transition-all disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center gap-2"
          >
            {isProcessing ? (
              <>
                <div className="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin" />
                <span>Processing...</span>
              </>
            ) : (
              <>
                <Shield className="w-4 h-4" />
                <span>Grant Access</span>
              </>
            )}
          </button>
        </div>
      </motion.div>
    </motion.div>
  );
}
