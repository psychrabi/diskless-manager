import { render, screen, fireEvent } from '@testing-library/react';
import { MemoryRouter, Routes, Route, Outlet, useNavigate } from 'react-router-dom';
import { vi, test, expect } from 'vitest';
import { AuthContext } from '@/contexts/auth';
import { checkAdminExists } from '@/api/modules/auth';
import PublicRoute from './PublicRoute';
vi.mock('@/api/modules/auth',()=>({checkAdminExists:vi.fn()}));
test('first installation automatically opens create administrator before login',async()=>{checkAdminExists.mockResolvedValue({exists:false});render(<AuthContext.Provider value={{loading:false,user:null,token:null}}><MemoryRouter initialEntries={['/login']}><Routes><Route path="/login" element={<PublicRoute>Login form</PublicRoute>}/><Route path="/initial-setup" element={<div>Create first administrator</div>}/></Routes></MemoryRouter></AuthContext.Provider>);expect(await screen.findByText('Create first administrator')).toBeInTheDocument();});

test('rechecks administrator existence after bootstrap before displaying login', async () => {
  let exists = false;
  checkAdminExists.mockImplementation(async () => ({ exists }));
  function Bootstrap() {
    const navigate = useNavigate();
    return <button onClick={() => { exists = true; navigate('/login'); }}>Create administrator</button>;
  }
  render(<AuthContext.Provider value={{loading:false,user:null,token:null}}>
    <MemoryRouter initialEntries={['/initial-setup']}><Routes>
      <Route element={<PublicRoute><Outlet /></PublicRoute>}>
        <Route path="/initial-setup" element={<Bootstrap />} />
        <Route path="/login" element={<div>Login form</div>} />
      </Route>
    </Routes></MemoryRouter>
  </AuthContext.Provider>);
  fireEvent.click(await screen.findByRole('button', { name: 'Create administrator' }));
  expect(await screen.findByText('Login form')).toBeInTheDocument();
});
