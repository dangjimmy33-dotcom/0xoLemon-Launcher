import { useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

export function useDefenderExclusion() {
  const [isDialogOpen, setIsDialogOpen] = useState(false);
  const [exclusionPath, setExclusionPath] = useState('');

  const checkExclusion = useCallback(async (path: string): Promise<boolean> => {
    try {
      return await invoke<boolean>('check_defender_exclusion', { path });
    } catch (err) {
      console.error('Failed to check defender exclusion:', err);
      return false;
    }
  }, []);

  const requestExclusion = useCallback(async (path: string) => {
    setExclusionPath(path);
    
    // Check if already excluded
    const isExcluded = await checkExclusion(path);
    if (isExcluded) {
      return { success: true, skipped: true };
    }

    // Show dialog
    return new Promise<{ success: boolean; skipped?: boolean }>((resolve) => {
      setIsDialogOpen(true);
      
      // Store resolve function for dialog callbacks
      (window as any).__defenderExclusionResolve = resolve;
    });
  }, [checkExclusion]);

  const handleAccept = useCallback(async () => {
    try {
      await invoke('add_defender_exclusion', { path: exclusionPath });
      const resolve = (window as any).__defenderExclusionResolve;
      if (resolve) {
        resolve({ success: true });
        delete (window as any).__defenderExclusionResolve;
      }
      setIsDialogOpen(false);
    } catch (err: any) {
      throw new Error(err?.message || 'Failed to add Windows Defender exclusion');
    }
  }, [exclusionPath]);

  const handleClose = useCallback(() => {
    const resolve = (window as any).__defenderExclusionResolve;
    if (resolve) {
      resolve({ success: false, skipped: true });
      delete (window as any).__defenderExclusionResolve;
    }
    setIsDialogOpen(false);
  }, []);

  return {
    isDialogOpen,
    exclusionPath,
    requestExclusion,
    checkExclusion,
    handleAccept,
    handleClose,
  };
}
