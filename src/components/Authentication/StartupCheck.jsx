import { useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { checkAdminExists } from "@/api/commands";
import { useAuth } from "@/contexts/auth";

const StartupCheck = () => {
  const navigate = useNavigate();
  const { token, loading: authLoading } = useAuth();

  useEffect(() => {
    if (authLoading) {
      // Wait for auth to finish loading
      return;
    }

    // If user is already logged in, redirect to app
    if (token) {
      navigate("/");
      return;
    }

    // Otherwise, check if admin user exists and redirect appropriately
    const checkSystemStatus = async () => {
      try {
        // Check if admin user exists
        const response = await checkAdminExists();
        const adminExists = response.exists || response.admin_exists;

        if (adminExists) {
          // Admin exists, go to login
          navigate("/login");
        } else {
          // No admin exists, go to initial setup
          navigate("/initial-setup");
        }
      } catch (error) {
        // If there's an error checking, assume no admin exists
        console.log(error);
        navigate("/initial-setup");
      }
    };

    checkSystemStatus();
  }, [navigate, token, authLoading]);

  return (
    <div className="flex items-center justify-center h-screen">
      <div className="text-center">
        <div className="inline-block animate-spin rounded-full h-12 w-12 border-t-2 border-b-2 border-primary"></div>
        <p className="mt-4 text-lg">Checking system status...</p>
      </div>
    </div>
  );
};

export default StartupCheck;