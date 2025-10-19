import React, { useEffect, useState } from 'react';
import { useForm } from 'react-hook-form';
import { invoke } from '@tauri-apps/api/core';

export default function DisksManagement() {
  const { register, handleSubmit, reset } = useForm({
    defaultValues: { zpool: '', name: '', usage_type: 'image' },
  });
  const [zpools, setZpools] = useState([]);
  const [datasets, setDatasets] = useState([]);
  const [loading, setLoading] = useState(false);
  const [msg, setMsg] = useState(null);
  const [err, setErr] = useState(null);
  const [selectedPool, setSelectedPool] = useState('');
  const [renameModalOpen, setRenameModalOpen] = useState(false);
  const [renameTarget, setRenameTarget] = useState(null);
  const [newName, setNewName] = useState('');

  // delete confirmation modal state
  const [deleteModalOpen, setDeleteModalOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState(null);
  const [deleteRecursive, setDeleteRecursive] = useState(true);

  async function fetchZpools() {
    try {
      const res = await invoke('list_zpools');
      setZpools(res || []);
      if ((res || []).length > 0 && !selectedPool) {
        setSelectedPool(res[0]);
      }
    } catch (e) {
      setErr(String(e));
    }
  }

  async function fetchDatasets(pool) {
    if (!pool) {
      setDatasets([]);
      return;
    }
    try {
      const res = await invoke('list_datasets', { zpool: pool });
      setDatasets(res || []);
    } catch (e) {
      setErr(String(e));
    }
  }

  useEffect(() => { fetchZpools(); }, []);

  useEffect(() => {
    if (selectedPool) {
      fetchDatasets(selectedPool);
      reset({ zpool: selectedPool, name: '', usage_type: 'image' });
    }
  }, [selectedPool]);

  const onSubmit = async (data) => {
    setErr(null); setMsg(null); setLoading(true);
    try {
      const resp = await invoke('create_zfs_dataset', { zpool: data.zpool, name: data.name, usageType: data.usage_type });
      setMsg(String(resp));
      await fetchDatasets(data.zpool);
      reset({ zpool: data.zpool, name: '', usage_type: 'image' });
    } catch (e) {
      setErr(e?.message ? String(e.message) : String(e));
    } finally { setLoading(false); }
  };

  // open delete confirmation modal
  function openDelete(dataset) {
    setDeleteTarget(dataset);
    setDeleteRecursive(true);
    setDeleteModalOpen(true);
  }

  async function confirmDelete() {
    if (!deleteTarget) return;
    setErr(null); setMsg(null); setLoading(true);
    try {
      await invoke('delete_zfs_dataset', { dataset: deleteTarget.name, recursive: deleteRecursive });
      setMsg(`Destroyed ${deleteTarget.name}`);
      setDeleteModalOpen(false);
      setDeleteTarget(null);
      await fetchDatasets(selectedPool);
    } catch (e) {
      setErr(e?.message ? String(e.message) : String(e));
    } finally {
      setLoading(false);
    }
  }

  async function handleDelete(dataset) {
    const confirmMsg = `Destroy dataset ${dataset.name}? This will delete all data under it.`;
    if (!window.confirm(confirmMsg)) return;
    setErr(null); setMsg(null);
    try {
      await invoke('delete_zfs_dataset', { dataset: dataset.name, recursive: true });
      setMsg(`Destroyed ${dataset.name}`);
      await fetchDatasets(selectedPool);
    } catch (e) {
      setErr(e?.message ? String(e.message) : String(e));
    }
  }

  function openRename(dataset) {
    setRenameTarget(dataset);
    // prefill newName with last path segment
    const parts = dataset.name.split('/');
    setNewName(parts[parts.length - 1]);
    setRenameModalOpen(true);
  }

  async function submitRename(e) {
    if (e && e.preventDefault) e.preventDefault();
    if (!renameTarget) return;
    const parent = renameTarget.name.split('/').slice(0, -1).join('/');
    const newFull = parent ? `${parent}/${newName}` : newName;
    if (!window.confirm(`Rename ${renameTarget.name} -> ${newFull}?`)) return;
    setErr(null); setMsg(null);
    try {
      await invoke('rename_zfs_dataset', { old: renameTarget.name, new: newFull });
      setMsg(`Renamed ${renameTarget.name} -> ${newFull}`);
      setRenameModalOpen(false);
      setRenameTarget(null);
      setNewName('');
      await fetchDatasets(selectedPool);
    } catch (e) {
      setErr(e?.message ? String(e.message) : String(e));
    }
  }

  useEffect(() => {
    if (selectedPool) {
      fetchDatasets(selectedPool);
      reset({ zpool: selectedPool, name: '', usage_type: 'image' });
    }
  }, [selectedPool]);

  return (
    <div className="card bg-base-100 p-4 shadow-md">
      <h3 className="text-lg font-semibold mb-4">Disks Management</h3>

      <form onSubmit={handleSubmit(onSubmit)} className="space-y-3">
        <label className="block">
          <span className="label-text">Zpool</span>
          <select
            className="select select-bordered w-full mt-1"
            {...register('zpool')}
            onChange={(e)=>setSelectedPool(e.target.value)}
            value={selectedPool}
          >
            <option value="">Select zpool</option>
            {zpools.map(p=> <option key={p} value={p}>{p}</option>)}
          </select>
        </label>

        <label className="block">
          <span className="label-text">Dataset name</span>
          <input className="input input-bordered w-full mt-1" {...register('name')} placeholder="e.g. images" />
        </label>

        <label className="block">
          <span className="label-text">Usage type</span>
          <select className="select select-bordered w-full mt-1" {...register('usage_type')}>
            <option value="image">Image (store images)</option>
            <option value="writeback">Writeback (store clones)</option>
            <option value="game">Game (second client target)</option>
          </select>
        </label>

        <div className="flex gap-2">
          <button className="btn btn-primary" type="submit" disabled={loading || !selectedPool}>{loading ? 'Creating...' : 'Create dataset'}</button>
          <button type="button" className="btn" onClick={()=> { reset(); setMsg(null); setErr(null); }}>Reset</button>
        </div>
      </form>

      {msg && <div className="mt-3 alert alert-success">{msg}</div>}
      {err && <div className="mt-3 alert alert-error">{err}</div>}

      <div className="mt-6">
        <h4 className="font-semibold mb-2">Datasets in {selectedPool || '—'}</h4>
        <div className="overflow-x-auto">
          <table className="table table-compact w-full">
            <thead><tr><th>Name</th><th>Type</th><th>Actions</th></tr></thead>
            <tbody>
              {datasets.map(ds=>(
                <tr key={ds.name}>
                  <td className="break-words max-w-xs">{ds.name}</td>
                  <td>{ds.disk_type || '—'}</td>
                  <td className="flex gap-2">
                    <button className="btn btn-sm" onClick={()=> openRename(ds)}>Rename</button>
                    <button className="btn btn-sm btn-error" onClick={()=> openDelete(ds)}>Delete</button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>

      {/* Rename modal (daisyUI) */}
      <input type="checkbox" id="rename-modal" className="modal-toggle" checked={renameModalOpen} readOnly />
      <div className={`modal ${renameModalOpen ? 'modal-open' : ''}`}>
        <div className="modal-box">
          <h3 className="font-bold text-lg">Rename dataset</h3>
          <p className="py-2">Renaming: <strong>{renameTarget?.name}</strong></p>
          <form onSubmit={submitRename} className="space-y-3">
            <label className="block">
              <span className="label-text">New name (last path segment)</span>
              <input value={newName} onChange={(e)=>setNewName(e.target.value)} className="input input-bordered w-full mt-1" />
            </label>
            <div className="modal-action">
              <button type="button" className="btn" onClick={()=> { setRenameModalOpen(false); setRenameTarget(null); setNewName(''); }}>Cancel</button>
              <button type="submit" className="btn btn-primary">Rename</button>
            </div>
          </form>
        </div>
      </div>

      {/* Delete confirmation modal */}
      <input type="checkbox" id="delete-modal" className="modal-toggle" checked={deleteModalOpen} readOnly />
      <div className={`modal ${deleteModalOpen ? 'modal-open' : ''}`}>
        <div className="modal-box">
          <h3 className="font-bold text-lg text-error">Confirm destroy dataset</h3>
          <p className="py-2">This will permanently delete <strong>{deleteTarget?.name}</strong> and all data under it.</p>
          <div className="form-control">
            <label className="cursor-pointer label">
              <span className="label-text">Destroy recursively (including children and snapshots)</span>
              <input type="checkbox" className="toggle toggle-primary ml-3" checked={deleteRecursive} onChange={(e)=>setDeleteRecursive(e.target.checked)} />
            </label>
          </div>
          <div className="modal-action">
            <button className="btn" onClick={()=> { setDeleteModalOpen(false); setDeleteTarget(null); }}>Cancel</button>
            <button className="btn btn-error" onClick={confirmDelete} disabled={loading}>{loading ? 'Deleting...' : 'Destroy'}</button>
          </div>
        </div>
      </div>
    </div>
  );
}