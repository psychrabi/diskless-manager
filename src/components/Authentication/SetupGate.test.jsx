import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { vi, test, expect, beforeEach } from 'vitest';
import { AuthContext } from '@/contexts/auth';
import { getSetupStatus } from '@/api/modules/system';
import SetupGate from './SetupGate';
import { AuthProvider } from '@/contexts/AuthContext';
import PublicRoute from './PublicRoute';
import { checkAdminExists, validateAuthToken } from '@/api/modules/auth';
vi.mock('@/api/modules/auth', () => ({ validateAuthToken: vi.fn().mockResolvedValue({}), checkAdminExists: vi.fn().mockResolvedValue({exists:true}) }));
vi.mock('@/api/modules/system', () => ({ getSetupStatus: vi.fn() }));
const mount = (role = 'admin', setup = false) => render(<AuthContext.Provider value={{user:{role},token:'valid',loading:false}}><MemoryRouter initialEntries={[setup ? '/setup' : '/']}><Routes><Route path="/" element={<SetupGate><div>Dashboard contents</div></SetupGate>}/><Route path="/setup" element={<SetupGate setup><div>Setup wizard</div></SetupGate>}/></Routes></MemoryRouter></AuthContext.Provider>);
beforeEach(() => { vi.resetAllMocks(); localStorage.clear(); checkAdminExists.mockResolvedValue({exists:true}); validateAuthToken.mockResolvedValue({}); });
test('does not render dashboard while readiness is pending', () => { getSetupStatus.mockReturnValue(new Promise(()=>{})); mount(); expect(screen.queryByText('Dashboard contents')).toBeNull(); });
test('incomplete server sends administrator to setup', async () => {getSetupStatus.mockResolvedValue({completed:false,ready:true,missing:[]});mount(); expect(await screen.findByText('Setup wizard')).toBeInTheDocument();expect(screen.queryByText('Dashboard contents')).toBeNull();});
test('status error blocks dashboard and retry checks again',async()=>{getSetupStatus.mockRejectedValueOnce(new Error('offline')).mockResolvedValue({completed:true,ready:true,missing:[]});mount();fireEvent.click(await screen.findByRole('button',{name:'Retry'}));expect(await screen.findByText('Dashboard contents')).toBeInTheDocument();});
test('non-admin cannot open incomplete setup wizard',async()=>{getSetupStatus.mockResolvedValue({completed:false,ready:false,missing:['storage']});mount('user',true);expect(await screen.findByText(/ask an administrator/i)).toBeInTheDocument();expect(screen.queryByText('Setup wizard')).toBeNull();});
test('marker with failed readiness still blocks dashboard', async()=>{getSetupStatus.mockResolvedValue({completed:true,ready:false,missing:['boot']});mount();await waitFor(()=>expect(screen.getByText('Setup wizard')).toBeInTheDocument());});

test('a session revoked by restore returns to login and clears saved credentials', async () => {
  localStorage.setItem('authToken', 'revoked-token');
  localStorage.setItem('user', JSON.stringify({role:'admin'}));
  getSetupStatus.mockRejectedValue(Object.assign(new Error('Session expired'), {status:401}));
  render(<AuthProvider><MemoryRouter><Routes>
    <Route path="/" element={<SetupGate>Dashboard contents</SetupGate>} />
    <Route path="/login" element={<PublicRoute><h1>Sign in</h1></PublicRoute>} />
  </Routes></MemoryRouter></AuthProvider>);
  expect(await screen.findByRole('heading', {name:'Sign in'})).toBeInTheDocument();
  expect(localStorage.getItem('authToken')).toBeNull();
  expect(localStorage.getItem('user')).toBeNull();
});
