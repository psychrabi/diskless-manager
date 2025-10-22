import React, { useEffect, useState, useCallback } from 'react';
import { useForm } from 'react-hook-form';
import { invoke } from '@tauri-apps/api/core';
import { Card } from '../ui';
import { File } from 'lucide-react';

export default function LicenseActivation() {
    const [info, setInfo] = useState({
        license_key: null,
        license_status: null,
        license_expires: null,
      });;

   // start with empty default and reset when async info arrives
   const { register, handleSubmit, reset } = useForm({
     defaultValues: { license_key: '' },
   });
  const [loading, setLoading] = useState(false);
  const [msg, setMsg] = useState(null);
  const [err, setErr] = useState(null);

  const fetchInfo = useCallback(async () => {
    try {
      const res = await invoke('get_license_info');
      console.log(res)
      const infoObj = res || {};
      setInfo(infoObj);
      // populate form input with license_key when fetched
      reset({ license_key: infoObj.license_key ?? '' });
    } catch (e) {
      setErr(String(e));
    }
  }, [reset]);

  useEffect(() => {
    fetchInfo();
  }, [fetchInfo]);

  const onSubmit = async (data) => {
    setErr(null);
    setMsg(null);
    if (!data.license_key || !data.license_key.trim()) {
      setErr('Please enter a license key');
      return;
    }
    setLoading(true);
    try {
      const resp = await invoke('activate_license', { key: data.license_key.trim() });
      setMsg(String(resp || 'License activated'));
      reset();
      await fetchInfo();
    } catch (e) {
      setErr(e?.message ? String(e.message) : String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
     <Card title="LIcense Activation" icon={File} >


      <form onSubmit={handleSubmit(onSubmit)} className="space-y-3">
        <label className="block">
          <span className="label-text">License</span>
          <input
            type="text"
            {...register('license_key')}            
            placeholder="Enter license key"
            className="input input-bordered w-full mt-1"
            readOnly={!!info.license_key} // make read-only if already activated
          />
        </label>

        <div className="flex items-center gap-2">
          <button type="submit" className="btn btn-primary" disabled={info.license_key || loading}>
            {loading ? 'Activating…' : 'Activate'}
          </button>
          <button
            type="button"
            className="btn"
            onClick={() => {
              reset();
              setMsg(null);
              setErr(null);
            }}
            disabled={loading}
          >
            Reset
          </button>
        </div>
      </form>

      {msg && <div className="mt-3 text-success">{msg}</div>}
      {err && <div className="mt-3 text-error">{err}</div>}
    </Card>
  );
}